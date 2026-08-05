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

pub fn lobby(identity: &AlphaIdentity) -> Response {
    let body = format!(
        r#"<section class="hero"><span class="eyebrow">PAPER RAID · 论文远征</span><h1>Research Lobby</h1><p>Welcome, {}. Matchmaking is waiting for the canonical Hepta adapter.</p></section>
        <section class="grid">
          {}
          {}
          {}
          {}
          {}
          {}
        </section>"#,
        escape(&identity.display_name),
        unavailable_card("Challenge / 研究挑战", "challenge catalog"),
        unavailable_card("Researchers / 队伍人数", "team count"),
        unavailable_card("Missing Role / 缺失职业", "role coverage"),
        unavailable_card("Ethics & License / 伦理与许可", "ethics and license facts"),
        unavailable_card("Budget / 预算", "budget facts"),
        unavailable_card("Queue / 匹配队列", "matchmaking contract"),
    );
    page("Paper Raid Lobby", &identity.display_name, &body)
}

pub fn formation(
    identity: &AlphaIdentity,
    team_id: &str,
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
        .map(|value| fact("Ready Check / 就绪确认", &value))
        .unwrap_or_else(|| unavailable_card("Ready Check / 就绪确认", "member acceptances"));
    let body = format!(
        r#"<section class="hero"><span class="eyebrow">TEAM FORMATION · 组队</span><h1>Research Cell</h1><p>Team <code>{}</code></p></section>
        <section class="panel"><h2>Players + Agents / 玩家与 Agent</h2><p class="source-state">Hepta: {}</p>{}</section>
        <section class="grid">{}{}{}</section>"#,
        escape(team_id),
        escape(team.label()),
        roster,
        unavailable_card("COI / 利益冲突", "COI declarations"),
        compact,
        readiness,
    );
    page("Paper Raid Formation", &identity.display_name, &body)
}

pub fn paper_room(
    identity: &AlphaIdentity,
    paper_id: &str,
    paper: ReadState<'_>,
    submission: ReadState<'_>,
    timeline: ReadState<'_>,
) -> Response {
    let title = paper
        .value()
        .and_then(|value| value.get("title"))
        .map(|value| scalar(Some(value)))
        .unwrap_or_else(|| "Unavailable".into());
    let phase = paper
        .value()
        .and_then(|value| value.get("phase"))
        .map(|value| scalar(Some(value)))
        .unwrap_or_else(|| "unavailable".into());
    let signoff = submission
        .value()
        .and_then(|value| value.get("author_consents"))
        .and_then(Value::as_array)
        .map(|values| format!("{} consent record(s)", values.len()))
        .unwrap_or_else(|| submission.label().into());
    let timeline_text = timeline
        .value()
        .and_then(|value| value.get("events"))
        .and_then(Value::as_array)
        .map(|values| format!("{} authoritative event(s)", values.len()))
        .unwrap_or_else(|| timeline.label().into());
    let settlement = match paper {
        ReadState::Available(_) => "<div class=\"status pending\">pending_finality</div>",
        ReadState::NotFound => "<div class=\"status missing\">not_found</div>",
        ReadState::Unavailable => "<div class=\"status missing\">unavailable</div>",
    };
    let settlement_fact = match paper {
        ReadState::Available(_) => "pending_finality",
        ReadState::NotFound => "not_found",
        ReadState::Unavailable => "unavailable",
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
        escape(paper.label()),
        settlement,
        fact("Phase Gates / 阶段门", &phase),
        fact("Author Sign-off / 作者签署", &signoff),
        fact("Timeline & Replay / 时间线与回放", &timeline_text),
        fact("Settlement / 结算", settlement_fact),
        unavailable_card(
            "Sections & Revisions / 章节与版本",
            "collaboration read model"
        ),
        unavailable_card("Evidence / 证据", "evidence cards"),
        unavailable_card("Claims / 论断", "claim ledger"),
        unavailable_card("Citations / 引用", "citation audit"),
        unavailable_card("Runs (including failed) / 全部实验", "run lineage"),
        unavailable_card("Figures / 图表", "figure lineage"),
        unavailable_card("Review / 内审", "review rounds"),
        unavailable_card("Reproduction / 复现", "reproduction reports"),
    );
    page("Paper Raid Room", &identity.display_name, &body)
}

pub fn login_page() -> Response {
    let body = r#"<section class="hero"><span class="eyebrow">PAPER RAID · ALPHA</span><h1>Research together.</h1><p>Use one of the three externally provisioned alpha login keys through the JSON login endpoint.</p></section>"#;
    page("Paper Raid Login", "not signed in", body)
}

fn page(title: &str, player: &str, content: &str) -> Response {
    let document = format!(
        r#"<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>{}</title><style>{}</style></head><body><header><a href="/league">HEPTA // PAPER RAID</a><span>{}</span></header><main>{}</main><footer>Hepta is the research record. Nakama is the ordered collaboration timeline.</footer></body></html>"#,
        escape(title),
        CSS,
        escape(player),
        content
    );
    let mut response = Html(document).into_response();
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(
            "default-src 'none'; style-src 'unsafe-inline'; form-action 'self'; base-uri 'none'; frame-ancestors 'none'",
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
main{margin:auto;max-width:1180px;padding:clamp(24px,5vw,64px) clamp(16px,4vw,44px)}.hero{border-left:4px solid var(--cyan);padding:8px 0 12px 22px;margin-bottom:28px}.eyebrow{color:var(--amber);font-size:12px;font-weight:800;letter-spacing:.16em}.hero h1{font-size:clamp(32px,7vw,72px);line-height:1;margin:10px 0}.hero p{color:var(--muted);max-width:760px}.grid{display:grid;grid-template-columns:repeat(3,minmax(0,1fr));gap:14px;margin:14px 0}.card,.panel{background:linear-gradient(145deg,#142036,#0d1421);border:1px solid var(--line);border-radius:14px;padding:18px;min-height:120px}.card h2,.panel h2{font-size:14px;letter-spacing:.05em;margin:0 0 12px}.card p{color:var(--text)}.unavailable{border-style:dashed;color:var(--muted)}.dot{background:var(--pink);border-radius:50%;display:inline-block;height:8px;margin-right:8px;width:8px}.status{border:1px solid var(--amber);border-radius:999px;color:var(--amber);display:inline-block;font-weight:800;padding:6px 12px}.status.missing{border-color:var(--pink);color:var(--pink)}.source-state{color:var(--muted);font-size:12px}.roster{display:grid;gap:10px;list-style:none;margin:0;padding:0}.roster li{align-items:center;border-bottom:1px solid var(--line);display:grid;gap:8px;grid-template-columns:90px 1fr 1fr 1fr;padding:10px 0}.roster span{color:var(--muted);overflow-wrap:anywhere}code{color:var(--cyan);overflow-wrap:anywhere}footer{padding:28px;text-align:center}
@media(max-width:820px){.grid{grid-template-columns:1fr}.roster li{align-items:start;grid-template-columns:1fr}.hero h1{font-size:38px}header{position:static}.card{min-height:auto}}
"#;

#[cfg(test)]
mod tests {
    use super::*;
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
    async fn paper_room_is_pending_only_and_missing_is_distinct() {
        let identity = AlphaIdentity::test_identity("subject-a", Uuid::new_v4(), Uuid::new_v4());
        let upstream_claim = format!("{}{}", "final", "ized");
        let paper = serde_json::json!({
            "phase":"submission_ready",
            "settlement": upstream_claim
        });
        let response = paper_room(
            &identity,
            "paper-a",
            ReadState::Available(&paper),
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

        let missing = paper_room(
            &identity,
            "paper-missing",
            ReadState::NotFound,
            ReadState::NotFound,
            ReadState::Unavailable,
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
}
