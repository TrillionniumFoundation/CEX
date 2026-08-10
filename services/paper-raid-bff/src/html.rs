use axum::{
    http::{header, HeaderValue},
    response::{Html, IntoResponse, Response},
};
use chrono::{DateTime, Utc};
use serde_json::Value;
use uuid::Uuid;

use crate::config::{AlphaAuthorRole, AlphaIdentity, AlphaIdentityScope};

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
                    let description = scalar(challenge.get("description"));
                    let gameplay = challenge_gameplay_summary(&description);
                    let status = scalar(challenge.get("status"));
                    let (role_choices, authorized_roles) = author_role_choices(identity);
                    format!(
                        r#"<article class="card challenge"><span class="pill">{}</span><h2>{}</h2>{}<code>{}</code><form class="queue-form" data-challenge-id="{}" data-authorized-roles="{}" data-agent-ready="{}"><fieldset class="role-kit"><legend>Role preferences / 角色偏好</legend><p class="muted">Checked roles are acceptable; the first listed role is your preferred assignment / 勾选可接受角色，首项为首选</p>{}</fieldset><label>Play window / 开局时间<select name="availability" required><option value="alpha-window">Join the next Alpha window / 下一场 Alpha</option><option value="now">Ready now / 现在可玩</option></select></label><p class="muted">{}</p><button type="submit" {}>Start my first Raid / 开始首局</button><output></output></form></article>"#,
                        escape(&status),
                        escape(&title),
                        gameplay,
                        escape(&challenge_id),
                        escape(&challenge_id),
                        escape(&authorized_roles),
                        queue_agent_ready,
                        role_choices,
                        if queue_agent_ready {
                            "Agent ready: exactly one active binding / Agent 已就绪"
                        } else {
                            "Pair exactly one active Agent before joining / 请先配对且仅保留一个活跃 Agent"
                        },
                        if authorized_roles.is_empty() || !queue_agent_ready {
                            "disabled"
                        } else {
                            ""
                        },
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
    r#"<section class="panel bridge-onboarding-primary agent-pairing-panel"><span class="pill">AGENT BRIDGE V2</span><h2>Pair an external Agent / 配对外部 Agent</h2><p>Generate one 5-minute pairing code in this authenticated browser, then enter it only at the local Bridge prompt. The BFF stores only its SHA-256 hash. The Bridge never receives your login key, browser cookie, CSRF token, or bearer credential, and never writes the code to argv, environment, config, state, or disk.</p><form class="agent-pairing-grant-form"><button type="submit">Generate one-time pairing code / 生成一次性配对码</button><output></output></form><div class="agent-pairing-code" hidden><p><strong>Shown once / 仅显示一次</strong> — copy it now; refreshing cannot recover it.</p><code class="agent-pairing-code-value"></code><button type="button" class="agent-pairing-copy">Copy code / 复制配对码</button></div><pre><code>node tools/paper-raid-agent-bridge/src/cli.mjs pair \
  --config paper-raid-agent-bridge.local.json</code></pre><p>The command reads the code from an interactive TTY, or from stdin when intentionally piped. It performs pairing-context discovery, AgentBinding V3 proof, authoritative recovery after a lost response, and persists only the resulting public binding state.</p><div class="agent-pairing-status"><button type="button" class="agent-pairing-refresh">Refresh pairing status / 刷新配对状态</button><button type="button" class="agent-pairing-revoke" hidden>Revoke unused code / 撤销未使用配对码</button><output></output></div></section>"#.to_string()
}

fn challenge_gameplay_summary(description: &str) -> String {
    let mut template = None;
    let mut template_version = None;
    let mut difficulty = None;
    let mut duration = None;
    let mut objective = None;
    let mut victory = None;
    let mut risk = None;
    let mut modifiers = None;
    let mut reward = None;
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
            "reward" => reward = Some(value),
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
        r#"<dl class="challenge-rules">{}<div><dt>Objective / 目标</dt><dd>{}</dd></div><div><dt>Victory / 胜利条件</dt><dd>{}</dd></div><div><dt>Risk / 主要风险</dt><dd>{}</dd></div><div><dt>Modifiers / 规则修饰</dt><dd>{}</dd></div><div><dt>Reward / 奖励</dt><dd>{}</dd></div></dl>"#,
        metadata,
        escape(objective),
        escape(victory),
        escape(risk.unwrap_or("challenge-specific integrity gates")),
        escape(modifiers.unwrap_or("standard-author-raid")),
        escape(reward.unwrap_or("non-economic progression")),
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
        "reproducing" => "Reproduce / 复现",
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
        .and_then(|paper| paper.get("phase"))
        .and_then(Value::as_str);
    let states = if paper_phase == Some("submission_ready") {
        ["complete", "complete", "complete", "complete", "complete"]
    } else if matches!(
        paper_phase,
        Some("integrity_review" | "reproducing" | "author_approval" | "integrity_hold")
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
            let phase = scalar(paper.get("phase"));
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
    let proposal_controls = match (proposal.value(), team.value(), proposal_status.as_str()) {
        (Some(_), None, "accepted") => format!(
            r#"<form class="materialize-team-form" data-proposal-id="{}" data-proposal-version="{}"><h3>Build the team / 建立队伍</h3><p>All three players accepted. Hepta will derive the exact roster and role contract.</p><button type="submit">Build our Research Cell / 建立正式队伍</button><output></output></form>"#,
            escape(resource_id),
            proposal_version,
        ),
        (Some(_), _, "proposed") => format!(
            r#"<form class="proposal-decision" data-proposal-id="{}" data-proposal-version="{}"><button name="decision" value="accept" type="submit">Accept Proposal / 接受组队</button><button class="danger" name="decision" value="decline" type="submit">Decline / 拒绝</button><output></output></form>"#,
            escape(resource_id),
            proposal_version,
        ),
        (Some(_), _, _) => String::new(),
        (None, _, _) => unavailable("team proposal"),
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
        <section class="grid"><article class="card"><h2>Match Proposal / 匹配提案</h2><span class="pill">{}</span>{}{}{}</article><article class="card"><h2>Players + Agents / 玩家与 Agent</h2><p class="source-state">Hepta Team: {}</p>{}</article>{}</section>
        <section class="panel"><h2>Formation Actions / 组队操作</h2><div class="action-grid">{}{}</div></section>"#,
        escape(resource_id),
        escape(&proposal_status),
        proposal_deadline,
        proposal_members,
        proposal_controls,
        escape(team.label()),
        roster,
        fact("Ready Check / 就绪确认", &readiness),
        compact,
        format!("{team_controls}{matched_ticket_control}"),
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
    let (settlement, settlement_fact) = finality_projection(review);
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
    let body = format!(
        r#"<section class="hero"><span class="eyebrow">PAPER ROOM · 论文作战室</span><h1>{}</h1><p>Paper <code>{}</code></p><p class="source-state">Hepta paper: {}</p>{}</section>{}
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
    let body = format!(
        "{body}<section class=\"grid\">{}{}{}{}{}{}</section>",
        review_card("Contribution / 贡献", "contribution_ledgers"),
        review_card("Evaluations / 评估", "evaluations"),
        review_card("Reproductions / 复现", "reproductions"),
        review_card("Appeals / 申诉", "appeals"),
        review_card("Resolutions / 裁决", "resolutions"),
        provisional_contribution_card(review),
    );
    let artifacts = artifact_links(room, paper_id);
    let body = format!(
        "{body}{artifacts}<details class=\"panel developer-tools\"><summary>Developer Tools / 开发者工具</summary><p class=\"muted\">Exact protocol JSON remains an Alpha fallback for commands that do not yet have a safely derived form or external connector. Needing this section is a known playability gap, never a completed normal path.</p><div class=\"action-grid\">{developer_actions}</div></details>"
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

fn finality_projection(review: ReadState<'_>) -> (String, String) {
    let fallback = match review {
        ReadState::NotFound => "not_found",
        ReadState::Unavailable | ReadState::Available(_) => "unavailable",
    };
    let Some(finality) = review
        .value()
        .and_then(|value| value.get("finality"))
        .filter(|value| value.is_object())
    else {
        return (
            format!("<div class=\"status missing\">{fallback}</div>"),
            fallback.into(),
        );
    };
    let Some(status) = finality.get("status").and_then(Value::as_str) else {
        return (
            "<div class=\"status missing\">unavailable</div>".into(),
            "unavailable".into(),
        );
    };
    if !matches!(status, "pending_finality" | "verified_finality") {
        return (
            "<div class=\"status missing\">unavailable</div>".into(),
            "unavailable".into(),
        );
    }
    let class = if status == "verified_finality" {
        "status verified"
    } else {
        "status pending"
    };
    let display = if status == "verified_finality" {
        "Verified scientific finality / 科学终局已验证"
    } else {
        "Verification pending / 等待终局验证"
    };
    let eligibility = [
        ("ranking_eligible", "Ranking / 排名"),
        ("reward_eligible", "Reward / 奖励"),
        ("score_eligible", "Score / 评分"),
        ("economic_eligible", "Economic / 经济"),
    ]
    .iter()
    .map(|(field, label)| {
        let eligible = finality
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
    (
        format!(
            r#"<div class="{}"><strong>{}</strong> · <code>{}</code></div><ul class="eligibility-grid">{}</ul>"#,
            class,
            display,
            escape(status),
            eligibility,
        ),
        format!("{display} ({status})"),
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
    let phase_controls = phase_transition_form(paper_id, paper_version, phase, progress);
    let next_actions = progress_actions(progress);
    let session_controls = research_session_panel(
        paper_id,
        paper_version,
        team_version,
        members,
        room_records(room, "member_research_sessions"),
        phase == "preregistering",
        !matches!(phase, "submission_ready" | "integrity_hold"),
    );
    let work_controls = work_item_panel(
        paper_id,
        paper_version,
        members,
        room_records(room, "work_items"),
        room_records(room, "artifact_manifests"),
        matches!(phase, "researching" | "drafting"),
        !matches!(phase, "submission_ready" | "integrity_hold"),
        &current_player_id,
        current_role,
        canonical_roles_enforced,
    );
    let science_controls = science_action_panel_for_role(
        paper_id,
        phase,
        room,
        current_role,
        canonical_roles_enforced,
    );
    let section_controls =
        section_collaboration_panel(paper_id, phase, room, members, &current_player_id);
    let revision_controls = revision_panel(
        paper_id,
        paper_version,
        paper,
        room,
        review,
        members,
        phase,
        &current_player_id,
    );
    let appeal_controls = author_appeal_panel(paper_id, phase, room, review);
    format!(
        r#"{}<section class="panel raid-command-center" data-paper-phase="{}"><div class="phase-objective"><span class="eyebrow">CURRENT OBJECTIVE / 当前目标</span><h2>{}</h2><p>{}</p><span class="pill">{} · {}</span>{}</div><div class="phase-readiness"><h3>Blockers / 阻塞项</h3><ul>{}</ul><p class="muted">Hepta's Author Raid projection is authoritative when present; local derivation is only a compatibility fallback.</p></div>{}</section>{}{}{}{}{}{}<section class="panel material-panel"><h2>Research Materials / 研究材料</h2><p>Upload immutable bytes here. A registered ArtifactManifest is still required before a revision can bind those bytes.</p><div class="action-grid">{}</div></section>"#,
        challenge_controls,
        escape(phase),
        escape(objective),
        escape(detail),
        escape(phase),
        escape(current_role),
        next_actions,
        blocker_items,
        phase_controls,
        session_controls,
        work_controls,
        science_controls,
        revision_controls,
        section_controls,
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
    format!(
        r#"<section class="panel challenge-ruleset-panel" data-ruleset-enforcement="{}" data-challenge-template="{}"><div class="challenge-ruleset-header"><div><span class="pill">AUTHORITATIVE RULESET</span><h2>Challenge Ruleset / 挑战规则</h2><p>The immutable Paper snapshot—not lobby copy or browser state—defines this Raid.</p></div><dl class="challenge-ruleset-facts"><div><dt>Template / 模式</dt><dd>{}</dd></div><div><dt>Difficulty / 难度</dt><dd>{}</dd></div><div><dt>Ruleset / 规则版本</dt><dd>{}</dd></div><div><dt>Raid clock / 挑战时长</dt><dd>{}</dd></div><div><dt>Grace / 宽限</dt><dd>{}</dd></div></dl></div>{}<div class="challenge-victory"><h3>Victory conditions / 胜利条件</h3><ul>{}</ul></div><details class="challenge-phase-gates"><summary>All authoritative phase gates / 全部权威阶段门</summary><ol>{}</ol></details>{}{}{}</section>"#,
        escape(enforcement),
        escape(template),
        escape(challenge_template_label(template)),
        escape(challenge_difficulty_label(template)),
        escape(ruleset_version),
        escape(&duration),
        escape(&grace_label),
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
    if seconds % 3_600 == 0 {
        format!("{}h", seconds / 3_600)
    } else if seconds % 60 == 0 {
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
        "Grace elapsed · Captain may record expired / 宽限已结束，队长可登记超时".into()
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
    grace: Option<DateTime<Utc>>,
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
    let expired = grace
        .map(|grace| {
            let grace_text = grace.to_rfc3339();
            let hidden = if Utc::now() >= grace { "" } else { " hidden" };
            format!(
                r#"<div class="expired-outcome-control" data-grace-expires-at="{}"{}><form class="challenge-outcome-form" data-paper-id="{}" data-confirm-message="Record expiry after the authoritative grace window? This terminal fact is immutable."><input type="hidden" name="outcome" value="expired"><input type="hidden" name="reason_code" value="grace_window_elapsed"><button class="danger" type="submit">Record expired / 登记超时</button><output></output></form></div>"#,
                escape(&grace_text),
                hidden,
                escape(paper_id),
            )
        })
        .unwrap_or_default();
    format!(
        r#"<details class="challenge-terminal-controls"><summary>End this Raid / 结束本次远征</summary><p class="muted">Use only when continuing is no longer scientifically valid. Hepta makes the selected outcome immutable.</p><div class="challenge-terminal-grid">{}{}{}</div></details>"#,
        failed, abandoned, expired,
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
        "grace_window_elapsed" => "Authoritative grace window elapsed / 权威宽限期已结束",
        _ => reason,
    }
}

fn challenge_transition_label(transition: &str) -> &'static str {
    match transition {
        "preregistering_to_researching" => "Preregistration → Research / 预注册 → 研究",
        "researching_to_experimenting" => "Research → Experiment / 研究 → 实验",
        "experimenting_to_drafting" => "Experiment → Draft / 实验 → 起草",
        "drafting_to_integrity_review" => "Draft → Integrity review / 起草 → 完整性审查",
        "integrity_review_to_reproducing" => "Integrity review → Reproduction / 完整性审查 → 复现",
        "reproducing_to_author_approval" => "Reproduction → Author approval / 复现 → 作者批准",
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
            "Prove reproducibility",
            "Preserve the independent reproduction result, or return the draft for rework.",
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
    review: Option<&Value>,
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
        "experimenting" => {
            if room_records(room, "runs").is_empty() {
                blockers.push(
                    "No run record is visible; failed runs count and must be retained.".into(),
                );
            }
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
        "integrity_review" => {
            if room_records(room, "section_reviews").is_empty() {
                blockers.push("No independent section review is visible.".into());
            }
        }
        "reproducing" => {
            let reproductions = review
                .map(|value| room_records(value, "reproductions"))
                .unwrap_or(&[]);
            if reproductions.is_empty() {
                blockers.push("No independent reproduction report is visible.".into());
            }
        }
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
                ("reproducing", "Begin reproduction / 开始独立复现"),
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
    let mut run_allowed = matches!(
        phase,
        "experimenting" | "drafting" | "integrity_review" | "reproducing"
    );
    if canonical_roles_enforced {
        evidence_allowed &= current_role == "evidence";
        plan_allowed &= current_role == "experiment";
        run_allowed &= current_role == "experiment";
    }
    let draft_bundle_allowed = matches!(phase, "drafting" | "reproducing");
    if !evidence_allowed && !plan_allowed && !run_allowed && !draft_bundle_allowed {
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
                r#"<form class="create-experiment-plan-form" data-paper-id="{}"><h3>Freeze experiment plan / 锁定实验计划</h3><p class="muted">Choose three distinct registered manifests. Descriptions are hashed locally; only digests leave this tab.</p><label>Protocol snapshot / 协议快照<textarea name="protocol" minlength="3" required placeholder="Describe the frozen protocol, or paste sha256:…"></textarea></label><label>Code manifest / 代码清单<select name="code_manifest_id" required><option value="">Choose code manifest / 选择代码清单</option>{}</select></label><label>Dataset manifest / 数据集清单<select name="dataset_manifest_id" required><option value="">Choose dataset manifest / 选择数据清单</option>{}</select></label><label>Environment manifest / 环境清单<select name="environment_manifest_id" required><option value="">Choose environment manifest / 选择环境清单</option>{}</select></label><label>Seed policy / 随机种子策略<textarea name="seed_policy" minlength="3" required placeholder="Describe deterministic seed selection"></textarea></label><label>Stopping rule / 停止规则<textarea name="stopping_rule" minlength="3" required placeholder="Describe when the experiment must stop"></textarea></label><button type="submit">Freeze plan / 锁定计划</button><output></output></form>"#,
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
            r#"<form class="create-evidence-card-form" data-paper-id="{}"><h3>Verify a source / 验证来源</h3><p class="muted">Your imported human key signs the exact server-built verification frame.</p><label>Canonical HTTPS source / 规范来源网址<input name="source_uri" type="url" maxlength="480" pattern="https://.*" required placeholder="https://example.org/paper"></label><label>Source bytes digest / 来源字节摘要<input name="source_hash" pattern="sha256:[0-9a-f]{{64}}" required placeholder="sha256:…"></label><label>Locator / 定位信息<input name="locator" maxlength="512" required placeholder="p. 3, Table 1"></label><label>License / 许可证<input name="license" maxlength="512" required value="CC-BY-4.0"></label><button type="submit">Verify and sign / 验证并签名</button><output></output></form>"#,
            escape(paper_id),
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
            r#"<form class="create-claim-record-form" data-paper-id="{}"><h3>Bind claim lineage / 绑定论断谱系</h3><p class="muted">The statement is hashed locally. Main, numeric, and figure claims require at least one selected lineage record.</p><label>Claim key / 论断标识<input name="claim_key" pattern="[A-Za-z0-9][A-Za-z0-9._:-]{{0,127}}" maxlength="128" required placeholder="primary-effect"></label><label>Claim type / 论断类型<select name="claim_kind" required><option value="main">Main / 核心</option><option value="numeric">Numeric / 数值</option><option value="figure">Figure / 图表</option><option value="supporting">Supporting / 支持</option><option value="limitation">Limitation / 局限</option></select></label><label>Statement / 论断<textarea name="statement" minlength="3" required placeholder="Write the exact claim, or paste sha256:…"></textarea></label><label>Evidence / 证据<select name="evidence_card_ids" multiple size="4">{}</select></label><label>Runs / 实验运行<select name="run_record_ids" multiple size="4">{}</select></label><label>Figures / 图表<select name="figure_lineage_ids" multiple size="4">{}</select></label><button type="submit">Bind claim / 绑定论断</button><output></output></form>"#,
            escape(paper_id),
            evidence_options,
            run_options,
            figure_options,
        ));
    }

    if run_allowed {
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
        if plan_options.is_empty() || manifest_options.is_empty() {
            actions.push_str(
                r#"<article class="guided-action blocked"><h3>Retain experiment run / 保留实验运行</h3><p>An experiment plan and registered log manifest are required / 需要实验计划与已登记日志清单</p></article>"#,
            );
        } else {
            actions.push_str(&format!(
                r#"<form class="create-run-record-form" data-paper-id="{}"><h3>Retain every run / 保留每次运行</h3><p class="muted">Failed and cancelled runs are first-class evidence. Descriptions are hashed locally.</p><label>Experiment plan / 实验计划<select name="experiment_plan_id" required><option value="">Choose plan / 选择计划</option>{}</select></label><label>Outcome / 结果<select name="status" required><option value="succeeded">Succeeded / 成功</option><option value="failed">Failed / 失败</option><option value="cancelled">Cancelled / 取消</option></select></label><label>Seed / 种子<input name="seed" type="number" step="1" value="1" required></label><label>Parameters / 参数<textarea name="parameters" minlength="1" required placeholder="Exact parameter set, or sha256:…"></textarea></label><label>Logs manifest / 日志清单<select name="logs_manifest_id" required><option value="">Choose logs / 选择日志</option>{}</select></label><label class="run-success-field">Outputs manifest / 输出清单<select name="outputs_manifest_id"><option value="">Choose outputs / 选择输出</option>{}</select></label><label class="run-success-field">Metrics / 指标<textarea name="metrics" placeholder="Metric summary, or sha256:…"></textarea></label><label class="run-failure-field" hidden>Failure record / 失败记录<textarea name="failure" placeholder="Failure reason, or sha256:…"></textarea></label><button type="submit">Retain run / 保留运行</button><output></output></form>"#,
                escape(paper_id),
                plan_options,
                manifest_options,
                manifest_options,
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
        }
    }

    format!(
        r#"<section class="panel science-action-panel"><h2>Scientific Actions / 科研行动</h2><p>Choose authoritative records from the room. The browser generates identifiers and sends only typed fields—never pasted JSON.</p><div class="action-grid">{}</div></section>"#,
        actions,
    )
}

fn work_item_panel(
    paper_id: &str,
    paper_version: u64,
    members: &[Value],
    work_items: &[Value],
    manifests: &[Value],
    primary: bool,
    editable: bool,
    current_player_id: &str,
    current_role: &str,
    canonical_roles_enforced: bool,
) -> String {
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
                    r#"<form class="bridge-proposal-task-form" data-paper-id="{}" data-section-key="{}" data-parent-revision-id="{}" data-binding-id="{}" data-agent-id="{}"><h3>Send task to local Agent Bridge / 发送任务至本地 Agent Bridge</h3><p class="muted">Keep <code>node tools/paper-raid-agent-bridge/src/cli.mjs inbox --config paper-raid-agent-bridge.local.json --watch</code> running on the Agent host. The Bridge reads its owner-only key, verifies this assignment, signs locally, and submits with <code>submit-proposal</code>. The browser only copies non-secret task parameters—never a signature JSON or private key.</p><dl><div><dt>Paper</dt><dd><code>{}</code></dd></div><div><dt>Section head</dt><dd><code>{}</code></dd></div><div><dt>Agent</dt><dd><code>{}</code></dd></div></dl><label>Assigned task / 已分配任务<select name="work_item_id" required>{}</select></label><label>Registered delivery artifact / 已登记交付工件<select name="artifact_manifest_id" required>{}</select></label><label>Delivery kind / 交付类型<select name="proposal_kind"><option value="delivery">Delivery / 交付</option><option value="proposal">Proposal / 建议</option></select></label><label>Agent output digest / Agent 输出摘要<input name="payload_hash" pattern="sha256:[0-9a-f]{{64}}" required placeholder="sha256:…"></label><button type="button" class="copy-bridge-proposal">Copy Bridge command / 复制 Bridge 命令</button><output></output></form>"#,
                    escape(paper_id),
                    escape(&section_key),
                    escape(parent_revision_id),
                    escape(binding_id),
                    escape(agent_id),
                    escape(paper_id),
                    escape(parent_revision_id),
                    escape(agent_id),
                    work_options,
                    manifest_options,
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

fn revision_panel(
    paper_id: &str,
    paper_version: u64,
    paper: &Value,
    room: &Value,
    review: Option<&Value>,
    members: &[Value],
    phase: &str,
    current_player_id: &str,
) -> String {
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
            r#"<form class="{}" data-paper-id="{}" data-paper-version="{}" data-parent-revision-id="{}"><h3>Freeze a paper revision / 锁定论文版本</h3><label>Registered manifest / 已登记清单<select name="manifest_source">{}</select></label><label>Paper source digest<input name="source_manifest_hash" pattern="sha256:[0-9a-f]{{64}}" placeholder="sha256:…" required></label><label>Artifact manifest digest<input name="artifact_manifest_hash" pattern="sha256:[0-9a-f]{{64}}" placeholder="sha256:…" required></label><label>Bibliography digest<input name="bibliography_hash" pattern="sha256:[0-9a-f]{{64}}" placeholder="sha256:…" required></label><label>Claim–evidence graph digest<input name="claim_evidence_graph_hash" pattern="sha256:[0-9a-f]{{64}}" placeholder="sha256:…" required></label><button type="submit">Create frozen revision / 创建锁定版本</button><output></output></form>"#,
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
    let mut author_fields = String::new();
    for member in members {
        let player_id = scalar(member.get("player_id"));
        let slot = scalar(member.get("participant_slot"));
        let role = scalar(member.get("role"));
        let credit_roles = match role.as_str() {
            "captain" => "conceptualization,project_administration,writing_review_editing",
            "evidence" => "investigation,data_curation,validation,writing_review_editing",
            "experiment" => "methodology,software,validation",
            _ => "investigation,writing_review_editing",
        };
        author_fields.push_str(&format!(
            r#"<fieldset class="release-author" data-player-id="{}" data-participant-slot="{}"><legend>Slot {} · {}</legend><label>Registered display name / 注册昵称<input name="display_name" maxlength="120" required></label><label>CRediT roles / 贡献角色<input name="credit_roles" value="{}" required></label></fieldset>"#,
            escape(&player_id),
            escape(&slot),
            escape(&slot),
            escape(&role),
            escape(credit_roles),
        ));
    }
    format!(
        r#"<form class="promote-release-form primary-action" data-paper-id="{}" data-paper-version="{}" data-revision-id="{}" data-revision-version="{}" data-collaboration-compact-hash="{}"><h3>Promote the exact release / 提升精确发布候选</h3><p class="muted">The browser deterministically budgets the zero-milestone contribution ledger from this exact author roster, freezes its hash into the release candidate, then immediately submits the same ledger. No player enters a ledger UUID, JSON, or hash.</p><label>Title / 标题<input name="title" value="{}" maxlength="200" required></label><label>Abstract / 摘要<textarea name="abstract_text" rows="5" minlength="20" required></textarea></label><label>License / 许可证<input name="license" value="CC-BY-4.0" required></label><fieldset><legend>Frozen disclosures / 锁定披露</legend><label>Research protocol digest<input name="research_protocol_snapshot_hash" value="{}" pattern="sha256:[0-9a-f]{{64}}" required></label><label>Ethics disclosure digest<input name="ethics_disclosure_hash" value="{}" pattern="sha256:[0-9a-f]{{64}}" required></label><label>Conflict-of-interest digest<input name="coi_disclosure_hash" pattern="sha256:[0-9a-f]{{64}}" required></label><label>AI disclosure digest<input name="ai_disclosure_hash" value="{}" pattern="sha256:[0-9a-f]{{64}}" required></label></fieldset><fieldset><legend>Exact author roster / 精确作者阵容</legend>{}</fieldset><button type="submit">Promote + freeze ledger / 提升并锁定贡献账本</button><output></output></form>"#,
        escape(paper_id),
        paper_version,
        escape(&revision_id),
        revision_version,
        escape(compact_hash),
        escape(&title),
        escape(protocol_hash),
        escape(&ethics_hash),
        escape(&ai_hash),
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
            r#"<span class="ledger-author" data-player-id="{}">{}</span>"#,
            escape(&player_id),
            roles,
        ));
    }
    if author_records.is_empty() {
        return unavailable("frozen release contribution roster");
    }
    format!(
        r#"<form class="freeze-contribution-ledger-form primary-action" data-paper-id="{}" data-paper-version="{}" data-revision-id="{}" data-release-candidate-hash="{}" data-expected-ledger-hash="{}"><p class="muted">Safe after a lost promote response or browser restart: the ledger UUID and hash are reconstructed from the frozen paper, revision, and release authors. Empty accepted-artifact/review references intentionally budget zero provisional points.</p><div class="ledger-author-records" hidden>{}</div><button type="submit">Repair / freeze exact ledger / 修复并锁定精确账本</button><output></output></form>"#,
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
        r#"<section class="hero"><span class="eyebrow">REVIEWER RAID · 独立评审</span><h1>Frozen-bundle Review Queue</h1><p>Author Raid ends at <code>submission_ready</code>. Independent evaluators, reviewers, and reproducers enter through this separate authority boundary.</p></section>
        <section class="panel review-boundary"><div><span class="pill">Independent authority / 独立权威</span><h2>Claim a precise role, never an Author Room</h2><p>Only a frozen, submission-ready PaperBundle is listed. Claiming creates a time-bounded assignment; it never grants team membership or access to an in-progress Paper Room.</p></div><dl><div><dt>Queue source</dt><dd>{}</dd></div><div><dt>Your player</dt><dd><code>{}</code></dd></div></dl></section>
        <section class="review-queue-grid">{}</section>
        <section class="panel"><span class="pill">PLAYER-SIGNED QUORUM</span><h2>Evaluation → two independent attestations → reproduction</h2><p>Each assigned actor sees only the frozen bundle and their own active assignment. Evaluation drafts and reviewer votes are immutable, locally human-signed records; the evaluator can finalize only after both reviewer slots attest to the exact same signing hash.</p></section>"#,
        escape(queue.label()),
        escape(&identity.player_id.to_string()),
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
        let release_hash = scalar(item.get("release_candidate_hash"));
        let bundle_hash = scalar(item.get("paper_bundle_hash"));
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
            r#"<article class="panel review-queue-card"><header><div><span class="eyebrow">FROZEN PAPERBUNDLE</span><h2>{}</h2></div><span class="pill">{} authors</span></header><p>{}</p><dl class="review-facts"><div><dt>Target</dt><dd>{}</dd></div><div><dt>Submitted</dt><dd>{}</dd></div><div><dt>Release hash</dt><dd><code>{}</code></dd></div><div><dt>PaperBundle hash</dt><dd><code>{}</code></dd></div></dl><div class="review-columns"><section><h3>My assignment / 我的任务</h3><ul class="review-assignments">{}</ul></section><section><h3>Open slots / 可领取角色</h3><div class="review-open-slots">{}</div></section></div></article>"#,
            escape(&title),
            escape(&author_count),
            escape(&abstract_text),
            escape(&target_format),
            escape(&submitted_at),
            escape(&release_hash),
            escape(&bundle_hash),
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

pub fn review_bundle(
    identity: &AlphaIdentity,
    queue_item: &Value,
    submission: &Value,
    review_state: ReadState<'_>,
) -> Response {
    let paper_id = scalar(submission.get("paper_project_id"));
    let submission_id = scalar(submission.get("submission_id"));
    let release_hash = scalar(submission.get("release_candidate_hash"));
    let bundle_hash = scalar(submission.get("paper_bundle_hash"));
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
    let raid_controls = review_raid_controls(identity, submission, review_state);
    let body = format!(
        r#"<section class="hero"><span class="eyebrow">REVIEW RAID · FROZEN BUNDLE</span><h1>{}</h1><p>{}</p><a class="button" href="/league/review">Back to queue / 返回评审队列</a></section>
        <section class="grid"><article class="panel"><h2>Your immutable assignment / 你的不可变任务</h2><ul class="review-assignments">{}</ul></article><article class="panel"><h2>Authority facts / 权威事实</h2><dl class="review-facts"><div><dt>Status</dt><dd>{}</dd></div><div><dt>Submission</dt><dd><code>{}</code></dd></div><div><dt>Paper</dt><dd><code>{}</code></dd></div></dl></article></section>
        <section class="panel"><span class="pill">FROZEN PAPERBUNDLE</span><h2>Review target / 评审对象</h2><dl class="review-facts"><div><dt>Target format</dt><dd>{}</dd></div><div><dt>License</dt><dd>{}</dd></div><div><dt>Release hash</dt><dd><code>{}</code></dd></div><div><dt>PaperBundle hash</dt><dd><code>{}</code></dd></div><div><dt>Source manifest</dt><dd><code>{}</code></dd></div><div><dt>Artifact manifest</dt><dd><code>{}</code></dd></div><div><dt>Bibliography</dt><dd><code>{}</code></dd></div><div><dt>Claim/evidence graph</dt><dd><code>{}</code></dd></div></dl><h3>Frozen author roster / 冻结作者阵容</h3><ul class="review-assignments">{}</ul></section>
        {}"#,
        escape(&scalar(candidate.get("title"))),
        escape(&scalar(candidate.get("abstract_text"))),
        assignments,
        escape(&status),
        escape(&submission_id),
        escape(&paper_id),
        escape(&scalar(candidate.get("target_format"))),
        escape(&scalar(candidate.get("license"))),
        escape(&release_hash),
        escape(&bundle_hash),
        escape(&scalar(candidate.get("source_manifest_hash"))),
        escape(&scalar(candidate.get("artifact_manifest_hash"))),
        escape(&scalar(candidate.get("bibliography_hash"))),
        escape(&scalar(candidate.get("claim_evidence_graph_hash"))),
        authors,
        raid_controls,
    );
    page(
        "Paper Raid Frozen Review Bundle",
        &identity.display_name,
        &body,
        true,
    )
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
        (None, None) if evaluator => controls.push_str(&evaluation_draft_form(&paper_id)),
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
                        r#"<section class="panel"><span class="pill">ATTESTATION RECORDED</span><h2>Your immutable reviewer decision is authoritative</h2><p>Reload and lost-response recovery use the quorum read model; no signature JSON needs to be copied or resubmitted.</p></section>"#,
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

fn evaluation_draft_form(paper_id: &str) -> String {
    format!(
        r#"<section class="panel"><span class="pill">EVALUATOR</span><h2>Freeze an immutable evaluation draft</h2><p>Score each fixed dimension, attest every hard gate from the frozen bundle, choose one reproducibility tolerance preset, and disclose conflicts in plain language. The browser hashes the disclosure; the BFF derives all bundle and score hashes.</p><form class="review-evaluation-draft-form" data-paper-id="{}">
        <fieldset><legend>Reference metric + tolerance / 参考指标与容差</legend><label>Metric key / 指标键<input name="metric_key" value="primary_effect" pattern="[A-Za-z0-9][A-Za-z0-9._:-]{{0,127}}" required></label><label>Frozen reference, micros / 参考值（百万分之一）<input name="reference_metric" type="number" step="1" value="0" required></label><label>Tolerance preset / 容差预设<select name="tolerance_preset" required><option value="relative">Balanced relative / 相对偏差</option><option value="absolute">Exact absolute / 绝对偏差</option><option value="statistical">Statistical evidence / 统计证据</option></select></label><label>Absolute max delta, micros<input name="absolute_delta" type="number" min="0" step="1" value="50000"></label><label>Relative max delta, bps<input name="relative_delta" type="number" min="0" max="10000" step="1" value="500"></label><label>Statistical minimum interval overlap, bps<input name="interval_overlap" type="number" min="0" max="10000" step="1" value="8000"></label><label>Statistical maximum effect delta, micros<input name="effect_delta" type="number" min="0" step="1" value="50000"></label><label>Statistical minimum p-value, micros<input name="p_value" type="number" min="0" max="1000000" step="1" value="50000"></label></fieldset>
        <fieldset><legend>Paper score, basis points / 论文评分</legend><label>Method rigor (max 2500)<input name="method_rigor_bps" type="number" min="0" max="2500" value="2000" required></label><label>Experiment/statistics (max 1500)<input name="experiment_statistics_bps" type="number" min="0" max="1500" value="1200" required></label><label>Reproducibility (max 1500)<input name="reproducibility_bps" type="number" min="0" max="1500" value="1200" required></label><label>Evidence/citations (max 1500)<input name="evidence_citations_bps" type="number" min="0" max="1500" value="1200" required></label><label>Value/originality (max 1500)<input name="value_originality_bps" type="number" min="0" max="1500" value="1200" required></label><label>Argument/expression (max 1000)<input name="argument_expression_bps" type="number" min="0" max="1000" value="800" required></label><label>Ethics/transparency (max 500)<input name="ethics_transparency_bps" type="number" min="0" max="500" value="400" required></label></fieldset>
        <fieldset><legend>Hard gates / 硬门槛</legend><p>Check only gates proven by the frozen bundle. Leave any failed or unproven gate unchecked; a scientifically honest not-eligible result is valid.</p><label><input name="citations_and_data_authentic" type="checkbox"> Citations and data authentic</label><label><input name="failed_runs_disclosed" type="checkbox"> Failed runs disclosed</label><label><input name="all_authors_consented" type="checkbox"> All authors consented</label><label><input name="core_claims_have_evidence" type="checkbox"> Core claims have evidence</label><label><input name="artifact_lineage_complete" type="checkbox"> Artifact lineage complete</label><label><input name="license_ethics_coi_complete" type="checkbox"> License, ethics, and COI complete</label></fieldset>
        <label>Conflict-of-interest attestation / 利益冲突声明<textarea name="coi_statement" rows="4" required placeholder="State no conflict, or disclose the exact relationship and mitigation."></textarea></label><button type="submit">Sign and freeze draft / 签名并锁定草案</button><output></output></form></section>"#,
        escape(paper_id),
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
            r#"<section class="panel"><span class="pill">REPRODUCTION RECORDED</span><h2>{}</h2><dl class="review-facts"><div><dt>Version</dt><dd>{}</dd></div><div><dt>Rule results</dt><dd>{}</dd></div><div><dt>Report hash</dt><dd><code>{}</code></dd></div></dl><p>The immutable authority record restores this result after reload or a lost response.</p></section>"#,
            escape(&scalar(report.get("status"))),
            escape(&scalar(report.get("version"))),
            report
                .get("rule_results")
                .and_then(Value::as_array)
                .map_or(0, Vec::len),
            escape(&scalar(report.get("report_hash"))),
        );
    }

    let metric_inputs = evaluation
        .get("reference_metrics_micros")
        .and_then(Value::as_object)
        .map(|metrics| {
            metrics
                .iter()
                .map(|(metric, reference)| format!(
                    r#"<label>{} · reference {}<input class="observed-metric" data-metric="{}" type="number" step="1" required></label>"#,
                    escape(metric), escape(&scalar(Some(reference))), escape(metric)
                ))
                .collect::<String>()
        })
        .unwrap_or_default();
    let statistical_inputs = evaluation
        .get("tolerance_policy")
        .and_then(|value| value.get("rules"))
        .and_then(Value::as_array)
        .map(|rules| rules.iter().filter_map(|rule| {
            if rule.get("kind").and_then(Value::as_str) != Some("statistical") { return None; }
            let metric = rule.get("metric").and_then(Value::as_str)?;
            Some(format!(r#"<fieldset class="statistical-metric" data-metric="{}"><legend>{} statistical evidence</legend><label>Interval overlap, bps<input name="interval_overlap_bps" type="number" min="0" max="10000" step="1" required></label><label>Effect delta, micros<input name="effect_delta_micros" type="number" step="1" required></label><label>P-value, micros<input name="p_value_micros" type="number" min="0" max="1000000" step="1" required></label></fieldset>"#, escape(metric), escape(metric)))
        }).collect::<String>())
        .unwrap_or_default();
    format!(
        r#"<section class="panel"><span class="pill">REPRODUCER</span><h2>Submit an independent reproduction</h2><p>Observed values are fixed-point micros. Environment, run manifest, seed set, and COI are typed semantic statements hashed locally; no digest or identifier is pasted.</p><form class="review-reproduction-form" data-paper-id="{}" data-evaluation-id="{}"><fieldset><legend>Observed metrics / 观测指标</legend>{}</fieldset>{}<label>Seed set and derivation / 随机种子集<textarea name="seed_statement" rows="3" required></textarea></label><label>Environment manifest / 环境清单<textarea name="environment_statement" rows="3" required></textarea></label><label>Run manifest and artifact lineage / 运行与产物清单<textarea name="run_manifest_statement" rows="4" required></textarea></label><label>Conflict-of-interest attestation / 利益冲突声明<textarea name="coi_statement" rows="3" required></textarea></label><button type="submit">Sign and submit reproduction / 签名并提交复现</button><output></output></form></section>"#,
        escape(paper_id),
        escape(&evaluation_id),
        metric_inputs,
        statistical_inputs,
    )
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
        return r#"<section class="panel resolver-appeal-status"><span class="pill">APPEAL RESOLVED</span><h2>Your independent decision is authoritative / 你的独立裁决已生效</h2><p>The read model restored the immutable resolution after reload or a lost response. No signature or protocol JSON needs to be resubmitted.</p></section>"#.into();
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
            r#"<li class="ticket-card"><div><code>{}</code><span class="pill">{}</span></div>{}{}{}</li>"#,
            escape(ticket_id),
            escape(status),
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
.key-vault{margin-bottom:18px;min-height:auto}.key-vault summary{color:var(--cyan);cursor:pointer;font-weight:800}.key-vault form{margin:14px 0}.human-key-status{display:block;margin-top:10px}.challenge-rules{display:grid;gap:7px;margin:12px 0}.challenge-rules div{border-bottom:1px solid var(--line);display:grid;gap:4px;padding:5px 0}.challenge-rules dt{color:var(--muted);font-size:11px}.challenge-rules dd{margin:0}
  .mission-board{display:grid;gap:18px;grid-template-columns:minmax(220px,.8fr) minmax(0,2fr);margin-bottom:20px}.mission-board h2{font-size:24px;letter-spacing:0;margin:6px 0}.raid-steps{display:grid;gap:8px;grid-template-columns:repeat(5,minmax(0,1fr));list-style:none;margin:0;padding:0}.raid-steps li{border:1px solid var(--line);border-radius:10px;display:grid;gap:8px;padding:12px}.raid-steps li>span{color:var(--muted);font-size:11px;font-weight:900}.raid-steps strong{display:block}.raid-steps p{color:var(--muted);font-size:11px;line-height:1.35;margin:4px 0 0}.raid-steps [data-step-state=current]{background:#12334a;border-color:var(--cyan)}.raid-steps [data-step-state=current]>span{color:var(--cyan)}.raid-steps [data-step-state=complete]{border-color:#3b8f78}.raid-steps [data-step-state=complete]>span{color:#67e8b5}.role-kit{border:1px solid var(--line);border-radius:10px;display:grid;gap:7px;margin:0;padding:12px}.role-kit legend{color:var(--amber);font-size:12px;font-weight:800;padding:0 6px}.role-kit span{color:var(--muted);font-size:12px}.role-kit strong{color:var(--text)}.role-choice{align-items:start;border:1px solid var(--line);border-radius:8px;display:grid;gap:9px;grid-template-columns:auto 1fr;margin:0;padding:9px}.role-choice input{margin-top:3px}.role-choice small{display:block;line-height:1.35;margin-top:3px}.role-choice[data-preference-rank="1"]{border-color:var(--cyan)}.advanced-action summary{color:var(--muted);cursor:pointer;font-weight:800}.advanced-action[open] summary{color:var(--cyan);margin-bottom:12px}.continue-raid{border-color:var(--cyan);margin-bottom:18px}.button{border:1px solid var(--cyan);border-radius:8px;color:var(--cyan);display:inline-block;font-weight:800;padding:10px 12px;text-decoration:none}.guided-action{border:1px solid var(--cyan);border-radius:12px;padding:16px}.guided-action.blocked{border-color:var(--amber)}.live-status{display:flex;flex-wrap:wrap;gap:8px;margin:12px 0}.live-status span{border:1px solid var(--line);border-radius:999px;padding:6px 10px}.live-connection[data-state=live]{border-color:#3b8f78;color:#67e8b5}.live-connection[data-state=catching-up],.live-connection[data-state=reconnecting]{border-color:var(--amber);color:var(--amber)}.live-participants,.live-events{display:grid;gap:8px;list-style:none;padding:0}.live-participants li,.live-events li{border:1px solid var(--line);border-radius:8px;padding:10px}.live-participants [data-connected=true]{color:#67e8b5}.live-participants [data-connected=false]{color:var(--muted)}
.raid-command-center{border-color:var(--cyan);display:grid;gap:20px;grid-template-columns:minmax(0,1.3fr) minmax(260px,.7fr);margin-bottom:18px}.phase-objective h2{font-size:clamp(22px,4vw,36px);letter-spacing:0;margin:7px 0}.phase-readiness ul{display:grid;gap:7px;margin:0 0 8px;padding-left:20px}.phase-readiness .ready{color:#67e8b5}.projected-actions{margin-top:14px}.projected-actions ul{display:flex;flex-wrap:wrap;gap:6px;list-style:none;margin:7px 0 0;padding:0}.projected-actions li{border:1px solid var(--line);border-radius:999px;color:var(--muted);font-size:11px;padding:4px 8px}.primary-action{background:#0b2637;border:1px solid var(--cyan);border-radius:12px;padding:16px}.work-item-panel,.research-session-panel,.revision-panel,.material-panel{margin:14px 0}.work-board{display:grid;gap:10px}.work-card,.session-card,.release-approval{border:1px solid var(--line);border-radius:10px;padding:14px}.work-card h3,.session-card h3{margin:8px 0}.accepted-artifact-field[hidden]{display:none}.session-control-stack{display:grid;gap:9px}.materialization-summary{border:1px solid #3b8f78;border-radius:10px;margin:12px 0;padding:14px}.materialization-summary.missing{border-color:var(--pink)}.materialization-summary.pending{border-color:var(--line)}.materialized-sections{display:grid;gap:7px;list-style:none;margin:10px 0 0;padding:0}.materialized-sections li{border-top:1px solid var(--line);display:grid;gap:4px;padding-top:8px}.materialized-sections span,.materialized-sections small{color:var(--muted)}.release-author{border:1px solid var(--line);border-radius:8px;display:grid;gap:8px;margin:9px 0;padding:10px}.release-author legend,fieldset>legend{color:var(--amber);font-size:12px;font-weight:800}.developer-tools{margin-top:22px}.developer-tools>summary{color:var(--muted);cursor:pointer;font-size:15px;font-weight:800}.developer-tools[open]>summary{color:var(--cyan);margin-bottom:12px}.eligibility-grid{display:grid;gap:5px;grid-template-columns:repeat(2,minmax(0,1fr));list-style:none;margin:10px 0 0;padding:0}.eligibility-grid li{border:1px solid var(--line);border-radius:7px;display:flex;font-size:11px;gap:6px;justify-content:space-between;padding:6px}.eligibility-grid [data-eligible=true]{border-color:#3b8f78;color:#67e8b5}.eligibility-grid [data-eligible=false]{color:var(--muted)}.status.verified{border-color:#3b8f78;color:#67e8b5}.ticket-list .ticket-card{align-items:stretch;display:grid;gap:8px}.ticket-card>div{display:flex;gap:8px;justify-content:space-between}.ticket-card form{display:block}.review-queue-empty{text-align:center}.review-queue-empty .button{margin-top:12px}.review-boundary{border-color:var(--cyan);display:grid;gap:18px;grid-template-columns:2fr 1fr;margin-bottom:18px}.review-boundary dl,.review-facts{display:grid;gap:7px;margin:0}.review-boundary dl div,.review-facts div{border-bottom:1px solid var(--line);display:grid;gap:4px;padding:7px 0}.review-boundary dt,.review-facts dt{color:var(--muted);font-size:11px}.review-boundary dd,.review-facts dd{margin:0;overflow-wrap:anywhere}.review-queue-grid{display:grid;gap:16px}.review-queue-card>header{align-items:start;background:none;border:0;display:flex;gap:12px;justify-content:space-between;padding:0;position:static}.review-queue-card>header h2{font-size:24px;letter-spacing:0;margin:6px 0 12px}.review-facts{grid-template-columns:repeat(2,minmax(0,1fr));margin:16px 0}.review-columns{display:grid;gap:14px;grid-template-columns:repeat(2,minmax(0,1fr))}.review-columns>section{border:1px solid var(--line);border-radius:10px;padding:13px}.review-columns h3{font-size:12px;margin:0 0 10px}.review-assignments{display:grid;gap:9px;list-style:none;margin:0;padding:0}.review-assignments li{align-items:center;display:flex;gap:10px;justify-content:space-between}.review-assignments span,.review-open-slots small{color:var(--muted);display:block;font-size:11px}.review-open-slots{display:grid;gap:8px}.review-claim-form{align-items:center;border-bottom:1px solid var(--line);display:grid;gap:8px;grid-template-columns:1fr auto;padding:8px 0}.review-claim-form output{grid-column:1/-1}.review-protocol-gap{border-color:var(--amber);margin-top:18px}.review-protocol-gap li{margin:6px 0}
.challenge-ruleset-panel{border-color:var(--amber);display:grid;gap:16px;margin-bottom:18px}.challenge-ruleset-header{display:grid;gap:16px;grid-template-columns:minmax(220px,1fr) minmax(300px,1.2fr)}.challenge-ruleset-header h2{font-size:24px;margin:8px 0}.challenge-ruleset-facts{display:grid;gap:6px;grid-template-columns:repeat(2,minmax(0,1fr));margin:0}.challenge-ruleset-facts div{border-bottom:1px solid var(--line);display:grid;gap:3px;padding:6px}.challenge-ruleset-facts dt{color:var(--muted);font-size:11px}.challenge-ruleset-facts dd{margin:0}.challenge-clock,.challenge-outcome,.challenge-victory{border:1px solid var(--line);border-radius:10px;padding:13px}.challenge-clock strong,.challenge-outcome strong{display:block;font-size:18px;margin-top:5px}.challenge-clock p,.challenge-outcome p{color:var(--muted);margin-bottom:0}.challenge-victory ul,.challenge-phase-gates ol,.challenge-phase-gates ul{display:grid;gap:6px;margin:8px 0;padding-left:22px}.challenge-phase-gates summary,.challenge-terminal-controls summary{color:var(--cyan);cursor:pointer;font-weight:800}.challenge-terminal-grid{display:grid;gap:12px;grid-template-columns:repeat(3,minmax(0,1fr));margin-top:12px}.challenge-terminal-grid form{border:1px solid var(--line);border-radius:10px;padding:12px}.expired-outcome-control[hidden]{display:none}.challenge-countdown[data-state=active]{color:#67e8b5}.challenge-countdown[data-state=overtime]{color:var(--amber)}.challenge-countdown[data-state=expired]{color:var(--pink)}
@media(max-width:820px){.grid,.action-grid,.mission-board,.raid-steps,.raid-command-center,.review-boundary,.review-facts,.review-columns,.challenge-ruleset-header,.challenge-ruleset-facts,.challenge-terminal-grid{grid-template-columns:1fr}.roster li{align-items:start;grid-template-columns:1fr}.hero h1{font-size:38px}header{position:static}.card{min-height:auto}}
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
        let identity = AlphaIdentity::test_identity("subject-a", Uuid::new_v4(), Uuid::new_v4());
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
        assert!(body.contains("data-step-state=\"current\"><span>01</span>"));
        assert!(body.contains("type=\"hidden\" value=\"captain,evidence,experiment\""));
        assert!(body.contains("Start my first Raid / 开始首局"));
        assert!(body.contains("data-agent-ready=\"false\""));
        assert!(body.contains("请先配对且仅保留一个活跃 Agent"));
        assert!(body.contains("<button type=\"submit\" disabled>Start my first Raid"));
        assert!(body.contains("class=\"cancel-ticket-form\""));
        assert!(body.contains(&format!("data-ticket-id=\"{ticket_id}\"")));
        assert!(body.contains("Leave queue / 取消排队"));
        assert!(body.contains("Compatible / 兼容人数: 1"));
        assert!(body.contains("Missing roles / 缺少角色: evidence, experiment"));
        assert!(body.contains("暂无法估算"));
        assert!(!body.contains("Roles / 职业<input"));
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
        let summary = challenge_gameplay_summary(
            "template=replication; template_version=v1; difficulty=advanced; duration_preset=120m; objective=reproduce the frozen baseline; victory=all hard gates pass; risk=environment drift; modifiers=frozen-environment,predeclared-tolerance; reward=non-economic:replication-mastery",
        );
        assert!(summary.contains("Template / 模式"));
        assert!(summary.contains("Template version / 模式版本</dt><dd>v1"));
        assert!(summary.contains("Difficulty / 难度</dt><dd>advanced"));
        assert!(summary.contains("Duration / 时长</dt><dd>120m"));
        assert!(summary.contains("Objective / 目标"));
        assert!(summary.contains("Victory / 胜利条件"));
        assert!(summary.contains("Risk / 主要风险</dt><dd>environment drift"));
        assert!(summary
            .contains("Modifiers / 规则修饰</dt><dd>frozen-environment,predeclared-tolerance"));
        assert!(summary.contains("non-economic:replication-mastery"));

        let escaped = challenge_gameplay_summary(
            "template=custom; objective=<script>alert(1)</script>; victory=do no harm",
        );
        assert!(!escaped.contains("<script>"));
        assert!(escaped.contains("&lt;script&gt;alert(1)&lt;/script&gt;"));
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
                "schema":"hepta.paper_raid.finality_projection.v1",
                "status":"verified_finality",
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
        assert!(panel.contains("Advanced / 高阶"));
        assert!(panel.contains("paper-raid-replication-v1"));
        assert!(panel.contains("Victory conditions / 胜利条件"));
        assert!(panel.contains("accepted work items / 已验收任务"));
        assert!(panel.contains("all author consents / 全部作者同意"));
        assert!(panel.contains("data-deadline-at=\"2099-08-10T10:00:00+00:00\""));
        assert!(panel.contains("data-grace-expires-at=\"2099-08-10T10:15:00+00:00\""));
        assert!(panel.contains("value=\"failed\""));
        assert!(panel.contains("value=\"abandoned\""));
        assert!(panel.contains("class=\"expired-outcome-control\" data-grace-expires-at=\"2099-08-10T10:15:00+00:00\" hidden"));
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
        assert!(body.contains("class=\"paper-phase-form primary-action\""));
        assert!(body.contains("value=\"preregistering\""));
        assert!(body.contains("Hepta projected objective"));
        assert!(body.contains("Coordinate the preregistration checkpoint."));
        assert!(body.contains("transition_paper_project"));
        assert!(body.contains("class=\"create-work-item-form\""));
        assert!(body.contains("class=\"issue-authorization-form\""));
        assert!(!body.contains("class=\"create-paper-revision-form\""));
        let developer = body.find("Developer Tools / 开发者工具").unwrap();
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
        assert!(body.contains("name=\"source_manifest_hash\""));
        assert!(body.contains("name=\"claim_evidence_graph_hash\""));
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
        let evidence_work = work_item_panel(
            "paper-role",
            3,
            members,
            work_items.as_array().expect("work items"),
            &[],
            true,
            true,
            &evidence_id.to_string(),
            "evidence",
            true,
        );
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
        assert!(!research.contains("Exact typed JSON"));

        let experimenting = science_action_panel("paper-a", "experimenting", &room);
        assert!(experimenting.contains("class=\"create-run-record-form\""));
        assert!(experimenting.contains("name=\"experiment_plan_id\""));
        assert!(!experimenting.contains("name=\"run_record_id\""));

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
        assert!(drafting.contains(&run_id.to_string()));
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
        assert_eq!(script.matches("sessionStorage").count(), 2);
        assert!(script.contains("sessionStorage.getItem(liveCursorKey(paperId))"));
        assert!(script.contains(
            "sessionStorage.setItem(liveCursorKey(paperId), JSON.stringify({ hepta: cursor.hepta }))"
        ));
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
        assert!(body.contains("Author Raid ends at <code>submission_ready</code>"));
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
        assert!(body.contains("PLAYER-SIGNED QUORUM"));
        assert!(body.contains("Evaluation → two independent attestations → reproduction"));
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
        let body = review_bundle(&evaluator, &base, &base, ReadState::Available(&state))
            .into_body()
            .collect()
            .await
            .expect("collect evaluator bundle")
            .to_bytes();
        let body = std::str::from_utf8(&body).expect("UTF-8 evaluator bundle");
        assert!(body.contains("review-evaluation-draft-form"));
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
        let body = review_bundle(
            &reproducer,
            &reproduction_bundle,
            &reproduction_bundle,
            ReadState::Available(&state),
        )
        .into_body()
        .collect()
        .await
        .expect("collect reproducer bundle")
        .to_bytes();
        let body = std::str::from_utf8(&body).expect("UTF-8 reproducer bundle");
        assert!(body.contains("review-reproduction-form"));
        assert!(body.contains("primary_effect · reference 1000000"));
        assert!(body.contains("Chain finality"));
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
            review_bundle(&reproducer, &bundle, &bundle, ReadState::Available(state))
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
        assert!(body.contains("paper-raid-agent-bridge/src/cli.mjs pair"));
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
