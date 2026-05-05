use super::*;

fn league_visible_copy(value: &str) -> String {
    let mut copy = value.to_string();
    let replacements = [
        ("Preseason Zero", "Preseason Zero / 预备赛季 Zero"),
        ("Founding Summoners", "Founding Summoners / 创世召唤者"),
        ("daily_dungeon", "Daily Dungeon / 每日副本"),
        ("bounty_arena", "Bounty Arena / 悬赏竞技场"),
        ("guild_raid", "Guild Raid / 公会团本"),
        ("face_to_face_duel", "Face Duel / 面对面切磋"),
        ("Prompt Forge 入门战", "Prompt Forge Starter Battle / Prompt Forge 入门战"),
        ("赏金赛：真实委托预备场", "Bounty Arena: Real Quest Tryout / 赏金赛：真实委托预备场"),
        ("公会团本：多阶段成果战", "Guild Raid: Multi-stage Result Battle / 公会团本：多阶段成果战"),
        ("附近对战：Agent Face Duel", "Nearby Duel: Agent Face Duel / 附近对战：Agent Face Duel"),
        ("用最少成本生成一个可提交成果，并给出自评/风险。", "Generate a submittable result at minimum cost, then add self-review and risk notes. / 用最少成本生成一个可提交成果，并给出自评/风险。"),
        ("多人提交方案，按质量、速度、成本和委托适配评分。", "Multiple players submit proposals scored by quality, speed, cost, and client fit. / 多人提交方案，按质量、速度、成本和委托适配评分。"),
        ("团队分工完成调研、构建、审核和成果提交。", "Split roles across research, build, audit, and result submission. / 团队分工完成调研、构建、审核和成果提交。"),
        ("像 Pokémon 近距离对战一样，面对面选择 Agent 阵容、出招、提交证据并结算奖励。", "Like a nearby Pokémon duel: choose Agent loadouts face to face, make moves, submit evidence, and settle rewards. / 像 Pokémon 近距离对战一样，面对面选择 Agent 阵容、出招、提交证据并结算奖励。"),
        ("open", "open / 开放"),
        ("preview", "preview / 预览"),
        ("XP + credits", "XP + credits / 经验 + 奖励点"),
        ("Prize Pool", "Prize Pool / 奖池"),
        ("Contribution split", "Contribution Split / 贡献分成"),
        ("Duel XP + rating", "Duel XP + rating / 切磋经验 + 段位分"),
        ("No loot yet", "No loot yet / 还没有掉落道具"),
        ("Apprentice", "Apprentice / 学徒"),
        ("Bronze I", "Bronze I / 青铜 I"),
        ("City Clerks", "City Clerks / 城市书记门"),
        ("Prompt Forge", "Prompt Forge / Prompt 锻造会"),
        (
            "Draft fast. Ship clean.",
            "Draft fast. Ship clean. / 快速组队，干净通关。",
        ),
        ("Audit Sanctum", "Audit Sanctum / 审稿圣所"),
        (
            "No hallucination survives the raid.",
            "No hallucination survives the raid. / 幻觉过不了团本审核。",
        ),
        ("rubric_scored", "rubric scored / 按规则评分"),
        ("eligible", "eligible / 可领奖"),
        ("review_hold", "review hold / 复核暂缓"),
        ("approved_release", "approved release / 已批准发放"),
        ("delivery_fit", "Deliverable fit / 成果适配"),
        ("evidence_grounding", "Evidence grounding / 证据扎实度"),
        ("risk_control", "Risk control / 风险控制"),
        ("actionability", "Next action / 下一步可执行性"),
        ("craft_polish", "Craft polish / 完成度"),
        ("hidden_tests", "Hidden tests / 隐藏测试"),
        ("llm_judge_adapter", "Judge adapter / 模型裁判"),
        ("pending", "pending / 待结算"),
        ("Level", "Level / 等级"),
        ("Skills/Tools/Skins", "Skills/Tools/Skins / 技能/工具/外观"),
        ("multi-agent", "multi-agent / 多 Agent"),
        ("Craft", "Craft / 工坊"),
        ("Market", "Market / 集市"),
        ("Assets", "Assets / 道具"),
        ("Events", "Events / 事件"),
        ("真实客户任务", "real global-client quest / 真实委托任务"),
        ("真实客户", "real global client / 海外真实委托"),
        ("客户适配", "client fit / 委托适配"),
        ("可交付方案", "submittable proposal / 可提交方案"),
        ("交付", "result submit / 成果提交"),
        ("AI 设计公司", "AI Design Studio / AI 设计工坊"),
        ("deliverable", "deliverable / 成果"),
        ("evidence", "evidence / 证据"),
        ("risk", "risk / 风险"),
        ("self-review", "self-review / 自评"),
        ("next action", "next action / 下一步"),
        ("Oracle Scout", "Oracle Scout / Oracle 侦察手"),
        ("Forge Builder", "Forge Builder / 锻造建造者"),
        ("Mirror Auditor", "Mirror Auditor / 镜像审稿人"),
        ("Courier Closer", "Courier Closer / 信使收尾人"),
        ("settled", "settled / 已结算"),
        ("held_review", "held review / 人工复核中"),
        ("rubric_hidden", "hidden rubric / 隐藏规则评分"),
        ("scout", "scout / 侦察"),
        ("builder", "builder / 建造"),
        ("auditor", "auditor / 审稿"),
        ("closer", "closer / 收尾"),
    ];
    if let Some((_, to)) = replacements.iter().find(|(from, _)| value == *from) {
        return (*to).to_string();
    }
    for (from, to) in replacements {
        copy = copy.replace(from, to);
    }
    copy
}

fn league_visible_copy_for_language(value: &str, language: &str) -> String {
    let copy = league_visible_copy(value);
    if copy.contains(" / ") {
        let pieces = copy
            .split(" / ")
            .map(str::trim)
            .filter(|piece| !piece.is_empty())
            .collect::<Vec<_>>();
        let preferred = if language == "zh" {
            pieces
                .iter()
                .copied()
                .find(|piece| contains_cjk_text(piece))
        } else {
            pieces
                .iter()
                .copied()
                .find(|piece| contains_latin_text(piece) && !contains_cjk_text(piece))
        };
        if let Some(piece) = preferred {
            return piece.to_string();
        }
    }
    if language != "zh" && contains_cjk_text(&copy) {
        value.to_string()
    } else {
        copy
    }
}

fn escape_league_visible_text(value: &str) -> String {
    let copy = league_visible_copy(value);
    i18n_span_from_bilingual_slash_copy(&copy).unwrap_or_else(|| escape_html_text(&copy))
}

pub(super) async fn get_league_season(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }
    let league = state.inner.league_state.lock().await;
    let mut players: Vec<LeaguePlayer> = league.players_by_matrix_user.values().cloned().collect();
    players.sort_by(|left, right| {
        right
            .rating
            .cmp(&left.rating)
            .then_with(|| right.xp.cmp(&left.xp))
            .then_with(|| left.matrix_user_id.cmp(&right.matrix_user_id))
    });
    let top_players: Vec<Value> = players
        .iter()
        .take(5)
        .map(|player| {
            json!({
                "player_id": player.player_id,
                "matrix_user_id": player.matrix_user_id,
                "display_name": player.display_name,
                "rating": player.rating,
                "xp": player.xp,
                "earned_credits": player.earned_credits,
            })
        })
        .collect();
    let guild_standings = league_guild_standings(&league);
    (
        StatusCode::OK,
        Json(json!({
            "kind": "league_season",
            "league": "trillionnium_league",
            "season": {
                "code": "preseason-zero",
                "name": "Preseason Zero",
                "status": "active",
                "theme": "Founding Summoners",
                "player_count": league.players_by_matrix_user.len(),
                "battle_count": league.battles.len(),
                "submission_count": league.submissions.len(),
                "reward_count": league.rewards.len(),
                "ops_contract_version": "trillionnium_season_ops_v1",
                "next_daily_reset_epoch": Utc::now().timestamp() + 86_400,
                "next_weekly_raid_epoch": Utc::now().timestamp() + 604_800,
                "market_refresh_policy": "restock sparse high-quality listings and apply demand/scarcity pricing",
                "scoreboard_reset_policy": "seasonal reset preserves earned inventory while refreshing leaderboards"
            },
            "ops_hooks": [
                {"hook_id": "daily_route_refresh", "cadence": "daily", "status": "scheduled_contract"},
                {"hook_id": "weekly_guild_raid_window", "cadence": "weekly", "status": "scheduled_contract"},
                {"hook_id": "market_supply_refresh", "cadence": "daily", "status": "scheduled_contract"},
                {"hook_id": "season_scoreboard_reset", "cadence": "seasonal", "status": "scheduled_contract"}
            ],
            "leaderboards": {
                "players": top_players,
                "guilds": guild_standings,
            }
        })),
    )
        .into_response()
}

pub(super) async fn get_league_web_shell(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Html<String> {
    let web_session = authorize_league_web_session_readonly(&state, &headers, true)
        .ok()
        .flatten();
    let csrf_input = web_session
        .as_ref()
        .map(|session| {
            format!(
                "<input type=\"hidden\" name=\"csrf\" value=\"{}\" />",
                escape_html_text(&session.csrf)
            )
        })
        .unwrap_or_default();
    let console_note = if web_session.is_some() {
        "Signed player session active: every action has CSRF protection and is bound to the current player. / 已进入签名玩家会话：所有行动都有 CSRF 防护，并绑定当前玩家。"
    } else if matches!(state.config().runtime_profile, RuntimeProfile::LocalDev) {
        "Local play lobby: join arenas, form teams, submit results, and settle rewards without exposing message or ledger tokens to the browser. / 本地互动大厅：可入场、组队、提交战果和结算奖励，不把消息或账本令牌暴露给浏览器。"
    } else {
        "Read-only lobby: create a signed /league/web/session before submitting actions. / 只读大厅：提交行动前需要先创建签名 /league/web/session。"
    };
    let console_note_html = i18n_span_from_bilingual_slash_copy(console_note)
        .unwrap_or_else(|| escape_html_text(console_note));
    let league = state.inner.league_state.lock().await;
    let mut matches: Vec<LeagueMatch> = league.matches.values().cloned().collect();
    matches.sort_by(|left, right| left.match_id.cmp(&right.match_id));
    let mut players: Vec<LeaguePlayer> = league.players_by_matrix_user.values().cloned().collect();
    players.sort_by(|left, right| {
        right
            .rating
            .cmp(&left.rating)
            .then_with(|| right.xp.cmp(&left.xp))
            .then_with(|| left.matrix_user_id.cmp(&right.matrix_user_id))
    });
    let total_rewards: f64 = league.rewards.iter().map(|reward| reward.amount).sum();
    let match_cards = matches
        .iter()
        .map(|league_match| {
            format!(
                "<article class=\"card match\"><div class=\"pill\">{}</div><h3>{}</h3><p>{}</p><footer><code>{}</code><span>{}</span></footer></article>",
                escape_league_visible_text(&league_match.mode),
                escape_league_visible_text(&league_match.title),
                escape_league_visible_text(&league_match.objective),
                escape_html_text(&league_match.match_id),
                escape_league_visible_text(&league_match.reward),
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let leaderboard = players
        .iter()
        .take(8)
        .enumerate()
        .map(|(idx, player)| {
            format!(
                "<tr><td>#{}</td><td>{}</td><td>{}</td><td>{}</td><td>{:.2}</td></tr>",
                idx + 1,
                escape_html_text(&player.display_name),
                escape_league_visible_text(&player.rank_tier),
                player.rating,
                player.earned_credits,
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let leaderboard = if leaderboard.is_empty() {
        "<tr><td>#1</td><td>@alice:local.dev</td><td><span data-i18n-en=\"Bronze I\" data-i18n-zh=\"青铜 I\">Bronze I</span></td><td>1000</td><td>0.00</td></tr>"
            .to_string()
    } else {
        leaderboard
    };
    let guild_cards = league
        .guilds
        .values()
        .map(|guild| {
            format!(
                "<article class=\"mini\"><strong>{}</strong><span>{}</span><code>{}</code></article>",
                escape_league_visible_text(&guild.name),
                escape_league_visible_text(&guild.motto),
                escape_html_text(&guild.guild_id),
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let mut timeline_items: Vec<(i64, String)> = Vec::new();
    for battle in league.battles.values() {
        timeline_items.push((
            battle.created_at_epoch,
            format!(
                "<li><b>⚔️ <span data-i18n-en=\"Battle\" data-i18n-zh=\"对战\">Battle</span></b><span>{}</span><small>{}</small></li>",
                escape_html_text(&battle.match_id),
                escape_html_text(&battle.task_id),
            ),
        ));
    }
    for submission in league.submissions.values() {
        timeline_items.push((
            submission.created_at_epoch,
            format!(
                "<li><b>🏁 <span data-i18n-en=\"Score\" data-i18n-zh=\"评分\">Score</span> {:.1}</b><span>{} · {} · {} <span data-i18n-en=\"events\" data-i18n-zh=\"项\">events</span></span><small>{}</small></li>",
                submission.score,
                escape_league_visible_text(&submission.grade),
                escape_league_visible_text(
                    submission
                        .judge_status
                        .as_deref()
                        .unwrap_or("rubric_scored")
                ),
                submission.score_events.len(),
                escape_html_text(&submission.submission_id),
            ),
        ));
    }
    for reward in &league.rewards {
        timeline_items.push((
            reward.created_at_epoch,
            format!(
                "<li><b>💰 +{:.2} {}</b><span>{}</span><small>{}</small></li>",
                reward.amount,
                escape_league_visible_text(&reward.currency_unit),
                escape_league_visible_text(reward.ledger_status.as_deref().unwrap_or("pending")),
                escape_html_text(&reward.reward_id),
            ),
        ));
    }
    timeline_items.sort_by(|left, right| right.0.cmp(&left.0));
    let timeline = timeline_items
        .into_iter()
        .take(10)
        .map(|(_, html)| html)
        .collect::<Vec<_>>()
        .join("\n");
    let timeline = if timeline.is_empty() {
        "<li><b><span data-i18n-en=\"No battle reports yet\" data-i18n-zh=\"还没有战报\">No battle reports yet</span></b><span data-i18n-en=\"Submit the first result to generate a replay.\" data-i18n-zh=\"提交第一份成果后会生成回放。\">Submit the first result to generate a replay.</span><small>/submit</small></li>"
            .to_string()
    } else {
        timeline
    };
    let loadout = league_loadout_for_player(&league, "@alice:local.dev");
    let player_items: Vec<&LeagueInventoryItem> = league
        .inventory_items
        .iter()
        .filter(|item| item.matrix_user_id == "@alice:local.dev")
        .collect();
    let top_loot = player_items
        .iter()
        .max_by(|left, right| left.power.cmp(&right.power))
        .map(|item| {
            format!(
                "{} ({})",
                league_visible_copy(&item.name),
                league_visible_copy(&item.rarity)
            )
        })
        .unwrap_or_else(|| "还没有掉落道具".to_string());
    let loadout_line = loadout
        .get("heroes")
        .and_then(Value::as_array)
        .map(|heroes| {
            heroes
                .iter()
                .filter_map(|hero| hero.get("name").and_then(Value::as_str))
                .map(escape_league_visible_text)
                .collect::<Vec<_>>()
                .join(" · ")
        })
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "侦察 Oracle · 锻造 Builder · 镜像 Auditor".to_string());
    let progression = league
        .players_by_matrix_user
        .get("@alice:local.dev")
        .map(|player| league_player_progression_json(&league, player, "@alice:local.dev"))
        .unwrap_or_else(|| {
            json!({
                "level": 1,
                "rank_title": "Apprentice",
                "successful_task_count": 0,
                "successes_to_next_level": 3,
                "experience_data_points": 0,
                "unlocked_skill_count": 0,
                "skill_count": league.league_skills.len(),
                "unlocked_tool_count": 0,
                "tool_count": league.league_tools.len(),
                "unlocked_skin_count": 0,
                "skin_count": league.league_skins.len(),
                "current_school": {"name": "City Clerks"},
            })
        });
    let progression_level = progression
        .get("level")
        .and_then(Value::as_i64)
        .unwrap_or(1);
    let progression_rank = progression
        .get("rank_title")
        .and_then(Value::as_str)
        .unwrap_or("Apprentice");
    let progression_school = progression
        .get("current_school")
        .and_then(|school| school.get("name"))
        .and_then(Value::as_str)
        .unwrap_or("City Clerks");
    let progression_successes = progression
        .get("successful_task_count")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let progression_data_points = progression
        .get("experience_data_points")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let unlocked_skill_count = progression
        .get("unlocked_skill_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let unlocked_tool_count = progression
        .get("unlocked_tool_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let unlocked_skin_count = progression
        .get("unlocked_skin_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let progression_rank_label_en = league_visible_copy_for_language(progression_rank, "en");
    let progression_school_label_en = league_visible_copy_for_language(progression_school, "en");
    let progression_rank_label_zh = league_visible_copy_for_language(progression_rank, "zh");
    let progression_school_label_zh = league_visible_copy_for_language(progression_school, "zh");
    let progression_line_en = format!(
        "Level {} {} · School {} · Successful quests {} · XP data {} · Skills/Tools/Skins {}/{}/{}",
        progression_level,
        progression_rank_label_en,
        progression_school_label_en,
        progression_successes,
        progression_data_points,
        unlocked_skill_count,
        unlocked_tool_count,
        unlocked_skin_count,
    );
    let progression_line_zh = format!(
        "等级 {} {} · 门派 {} · 成功任务 {} · 经验数据点 {} · 技能/工具/外观 {}/{}/{}",
        progression_level,
        progression_rank_label_zh,
        progression_school_label_zh,
        progression_successes,
        progression_data_points,
        unlocked_skill_count,
        unlocked_tool_count,
        unlocked_skin_count,
    );
    let latest_submission = league
        .submissions
        .values()
        .max_by_key(|submission| submission.created_at_epoch);
    let score_breakdown_cards = latest_submission
        .map(|submission| {
            submission
                .score_events
                .iter()
                .map(|event| {
                    let weighted_points = event.score * event.weight;
                    let width = event.score.clamp(0.0, 100.0);
                    format!(
                        "<article class=\"mini score-mini\"><strong>{}</strong><span>{:.1}/100 · weight {:.0}% · +{:.1} pts</span><div class=\"score-bar\" aria-label=\"score {:.1}\"><i style=\"width:{:.1}%\"></i></div><small>{}</small></article>",
                        escape_league_visible_text(&event.dimension),
                        event.score,
                        event.weight * 100.0,
                        weighted_points,
                        event.score,
                        width,
                        escape_league_visible_text(&event.judge_kind),
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        })
        .filter(|html| !html.is_empty())
        .unwrap_or_else(|| {
            "<article class=\"mini score-mini\"><strong data-i18n-en=\"No scored submission yet\" data-i18n-zh=\"还没有评分成果\">No scored submission yet</strong><span data-i18n-en=\"Submit a result to see the rubric breakdown.\" data-i18n-zh=\"提交成果后会显示评分拆解。\">Submit a result to see the rubric breakdown.</span><code>/submit daily-dungeon-001 &lt;result&gt;</code></article>".to_string()
        });
    let latest_rating_line_en = latest_submission
        .map(|submission| {
            format!(
                "Latest rating: {:.1}/100 · Grade {} · Reward {:.2} credits · Status {}",
                submission.score,
                league_visible_copy_for_language(&submission.grade, "en"),
                submission.reward_amount,
                league_visible_copy_for_language(
                    submission.payout_status.as_deref().unwrap_or("eligible"),
                    "en",
                ),
            )
        })
        .unwrap_or_else(|| "Latest rating appears after the first submitted result.".to_string());
    let latest_rating_line_zh = latest_submission
        .map(|submission| {
            format!(
                "最近评分：{:.1}/100 · 等级 {} · 奖励 {:.2} 点 · 状态 {}",
                submission.score,
                league_visible_copy_for_language(&submission.grade, "zh"),
                submission.reward_amount,
                league_visible_copy_for_language(
                    submission.payout_status.as_deref().unwrap_or("eligible"),
                    "zh",
                ),
            )
        })
        .unwrap_or_else(|| "提交第一份成果后会出现最近评分。".to_string());
    let reward_formula_en = "Reward = score ÷ 20 × mode multiplier. Multipliers: Daily 1.0×, Guild Raid 1.25×, Bounty Arena 1.5×. Anti-cheat flags hold payout for review instead of deleting progress.";
    let reward_formula_zh = "奖励 = 分数 ÷ 20 × 玩法倍率。倍率：每日副本 1.0×、公会团本 1.25×、悬赏竞技场 1.5×。反作弊命中时暂缓发放但不抹掉进度。";

    let league_header_language_switcher =
        trillionnium_language_inline_switcher_html("trillionnium-league-language-select");

    Html(format!(
        r#"<!doctype html>
<html lang="zh-CN">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <title>Trillionnium League</title>
  <style>
    :root {{ color-scheme: dark; --bg:#060711; --panel:#111426; --panel2:#171b31; --gold:#f8c35b; --cyan:#64e3ff; --pink:#ff5ca8; --text:#f6f7fb; --muted:#9aa3b2; }}
    * {{ box-sizing:border-box; }}
    body {{ margin:0; min-height:100vh; overflow-x:hidden; font-family:Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; background:radial-gradient(circle at 20% 0%, #213064 0, transparent 32rem), radial-gradient(circle at 88% 14%, #532044 0, transparent 30rem), var(--bg); color:var(--text); }}
    header {{ padding:42px min(6vw,72px) 18px; display:grid; gap:22px; grid-template-columns:1.25fr .75fr; align-items:end; }}
    h1 {{ margin:0; font-size:clamp(42px,7vw,92px); line-height:.9; letter-spacing:-.07em; }}
    h2 {{ margin:0 0 16px; letter-spacing:-.03em; }}
    .subtitle {{ color:var(--muted); font-size:18px; max-width:760px; }}
    .hero-card,.card,.panel {{ min-width:0; border:1px solid rgba(255,255,255,.11); background:linear-gradient(145deg,rgba(255,255,255,.09),rgba(255,255,255,.035)); box-shadow:0 24px 80px rgba(0,0,0,.35); backdrop-filter: blur(14px); border-radius:24px; }}
    .hero-card {{ padding:24px; display:grid; gap:14px; }}
    .stats {{ display:grid; grid-template-columns:repeat(5,minmax(0,1fr)); gap:14px; margin-top:22px; }}
    .stat {{ padding:18px; background:rgba(255,255,255,.06); border-radius:18px; }}
    .stat b {{ display:block; font-size:26px; color:var(--gold); }}
    main {{ padding:20px min(6vw,72px) 60px; display:grid; gap:24px; min-width:0; }}
    main > * {{ min-width:0; max-width:100%; }}
    main > section {{ order:8; }}
    .stats {{ order:1; }}
    #league-playable-modes {{ order:2; }}
    #league-battle-console {{ order:3; }}
    #league-progression {{ order:4; }}
    #league-world-bridge {{ order:5; }}
    .grid {{ display:grid; grid-template-columns:repeat(3,minmax(0,1fr)); gap:18px; }}
    .card {{ padding:20px; min-height:210px; overflow-wrap:anywhere; }}
    .card h3 {{ margin:12px 0; font-size:24px; }}
    .card p {{ color:var(--muted); line-height:1.55; }}
    .card footer {{ display:flex; justify-content:space-between; gap:10px; align-items:center; flex-wrap:wrap; margin-top:18px; color:var(--gold); }}
    .pill {{ display:inline-flex; border:1px solid rgba(100,227,255,.35); color:var(--cyan); padding:5px 10px; border-radius:999px; font-size:12px; letter-spacing:.12em; }}
    .league-hero-kicker {{ display:flex; align-items:center; justify-content:space-between; gap:12px; flex-wrap:wrap; }}
    .language-switcher {{ display:inline-flex; align-items:center; gap:8px; width:max-content; max-width:100%; border:1px solid rgba(100,227,255,.24); background:rgba(255,255,255,.065); color:var(--cyan); border-radius:999px; padding:6px 8px 6px 10px; font-size:12px; font-weight:900; }}
    .language-switcher select {{ width:auto; min-height:40px; min-width:92px; max-width:130px; margin:0; border:0; background:rgba(7,8,20,.72); color:var(--text); border-radius:999px; padding:7px 26px 7px 10px; font:inherit; font-size:12px; }}
    .panel {{ padding:24px; }}
    table {{ width:100%; border-collapse:collapse; table-layout:fixed; }}
    td,th {{ padding:12px 10px; border-bottom:1px solid rgba(255,255,255,.08); text-align:left; overflow-wrap:anywhere; }}
    th {{ color:var(--muted); font-weight:600; }}
    .commands {{ display:flex; flex-wrap:wrap; gap:10px; }}
    .play {{ display:grid; grid-template-columns:1fr 1fr; gap:18px; align-items:start; }}
    #league-battle-console .timeline {{ max-height:520px; overflow:auto; padding-right:4px; }}
    form {{ display:grid; gap:10px; margin:0; }}
    input,textarea,select {{ width:100%; color:var(--text); background:rgba(255,255,255,.07); border:1px solid rgba(255,255,255,.14); border-radius:14px; padding:12px 14px; font:inherit; }}
    textarea {{ min-height:92px; resize:vertical; }}
    button {{ border:0; cursor:pointer; color:var(--bg); background:linear-gradient(135deg,var(--gold),#ff8d4d); padding:12px 16px; border-radius:14px; font-weight:800; }}
    .mini-grid {{ display:grid; grid-template-columns:repeat(2,minmax(0,1fr)); gap:12px; }}
    .mini {{ display:grid; gap:7px; padding:14px; border-radius:16px; background:rgba(255,255,255,.06); border:1px solid rgba(255,255,255,.08); }}
    .mini span, .timeline small {{ color:var(--muted); }}
    .score-grid {{ display:grid; grid-template-columns:repeat(3,minmax(0,1fr)); gap:10px; margin-top:12px; max-height:250px; overflow:auto; padding-right:4px; }}
    .score-mini {{ align-content:start; padding:11px; gap:5px; }}
    .score-bar {{ height:7px; border-radius:999px; overflow:hidden; background:rgba(255,255,255,.09); }}
    .score-bar i {{ display:block; height:100%; border-radius:inherit; background:linear-gradient(90deg,var(--cyan),var(--gold)); }}
    .reward-formula {{ display:grid; gap:6px; padding:12px; border-radius:16px; background:rgba(248,195,91,.08); border:1px solid rgba(248,195,91,.2); color:var(--muted); }}
    .score-breakdown-drawer {{ margin-top:10px; }}
    .score-breakdown-drawer summary {{ min-height:44px; display:flex; align-items:center; font-weight:800; color:var(--cyan); cursor:pointer; }}
    .timeline {{ list-style:none; padding:0; margin:0; display:grid; gap:10px; }}
    .timeline li {{ display:grid; grid-template-columns:1.3fr .6fr 1.1fr; gap:10px; padding:12px; border-radius:14px; background:rgba(255,255,255,.055); }}
    code {{ color:var(--cyan); background:rgba(100,227,255,.08); padding:3px 7px; border-radius:8px; overflow-wrap:anywhere; word-break:break-word; }}
    .cta {{ color:var(--bg); background:linear-gradient(135deg,var(--gold),#ff8d4d); padding:14px 18px; border-radius:16px; display:inline-flex; justify-content:center; align-items:center; min-height:48px; font-weight:800; text-decoration:none; }}
    .cta.secondary {{ color:var(--text); background:rgba(255,255,255,.07); border:1px solid rgba(100,227,255,.22); }}
    .league-hero-actions {{ display:grid; grid-template-columns:repeat(3,minmax(0,1fr)); gap:10px; }}
    @media (max-width:900px) {{ header {{ grid-template-columns:1fr; padding:18px 16px 8px; gap:12px; }} h1 {{ font-size:clamp(38px,13vw,58px); }} .subtitle {{ font-size:14px; line-height:1.42; }} header > section .subtitle {{ margin:8px 0 0; display:-webkit-box; -webkit-line-clamp:2; -webkit-box-orient:vertical; overflow:hidden; }} main > section {{ order:8; }} .grid,.play,.mini-grid {{ grid-template-columns:minmax(0,1fr); }} .score-grid {{ grid-template-columns:repeat(2,minmax(0,1fr)); max-height:220px; }} .score-mini small {{ display:none; }} .stats {{ order:1; display:grid; grid-template-columns:repeat(3,minmax(0,1fr)); overflow:visible; gap:8px; margin-top:8px; padding-bottom:0; }} .stat {{ min-height:68px; min-width:0; padding:10px 8px; }} .stat b {{ font-size:clamp(17px,5.4vw,22px); letter-spacing:-.03em; }} .stat span {{ font-size:11px; }} #league-playable-modes {{ order:2; }} #league-playable-modes .grid {{ grid-template-columns:repeat(2,minmax(0,1fr)); gap:8px; }} #league-playable-modes .card {{ display:grid; gap:5px; min-height:0; padding:11px; }} #league-playable-modes .card h3 {{ margin:2px 0; font-size:15px; display:-webkit-box; -webkit-line-clamp:2; -webkit-box-orient:vertical; overflow:hidden; }} #league-playable-modes .card p {{ display:none; }} #league-playable-modes .card footer {{ gap:5px; margin-top:2px; font-size:10px; }} #league-playable-modes .pill {{ padding:4px 7px; font-size:9px; letter-spacing:.08em; }} #league-battle-console {{ order:3; }} #league-battle-console .panel {{ padding:18px; }} #league-battle-console textarea {{ min-height:70px; }} #league-battle-console .timeline {{ max-height:320px; overflow:auto; padding-right:4px; }} #league-battle-console .timeline li {{ grid-template-columns:1fr; gap:6px; padding:10px; }} #league-progression {{ order:4; }} #league-world-bridge {{ order:5; }} .card {{ min-height:auto; padding:16px; }} .card h3 {{ font-size:20px; line-height:1.15; }} .card p {{ margin:8px 0; line-height:1.45; }} .card footer {{ font-size:12px; }} .hero-card {{ padding:16px; border-radius:20px; gap:10px; }} .hero-card .subtitle {{ margin:0; display:-webkit-box; -webkit-line-clamp:2; -webkit-box-orient:vertical; overflow:hidden; }} .league-hero-actions {{ grid-template-columns:repeat(3,minmax(0,1fr)); gap:7px; }} .league-hero-actions .cta {{ font-size:12px; }} .cta {{ min-height:44px; padding:10px 12px; border-radius:14px; }} .language-switcher {{ padding:5px 6px 5px 8px; font-size:11px; }} .language-switcher select {{ min-height:40px; min-width:82px; max-width:112px; padding:6px 22px 6px 8px; font-size:11px; }} }}
  </style>
</head>
<body>
  <header>
    <section>
      <div class="league-hero-kicker"><div class="pill"><span data-i18n-en="Global-first Beta" data-i18n-zh="海外市场首发">Global-first Beta</span> · Preseason Zero</div><div id="league-language-switcher">{league_header_language_switcher}</div></div>
      <h1>Trillionnium League</h1>
      <p class="subtitle" data-i18n-en="AI Agent arena for overseas-first launch: draft your squad, enter dungeons, clear bounties, submit results, get rated, rank up, and earn rewards." data-i18n-zh="面向海外首发的 AI Agent 竞技场：组建队伍、进入副本、完成悬赏、提交成果、获得评分、升级段位并领取奖励。">AI Agent arena for overseas-first launch: draft your squad, enter dungeons, clear bounties, submit results, get rated, rank up, and earn rewards.</p>
    </section>
    <aside class="hero-card">
      <strong data-i18n-en="Playable Now" data-i18n-zh="当前可玩版本">Playable Now</strong>
      <p class="subtitle" data-i18n-en="Pick a mode, draft your Agent squad, submit a result, then watch rewards and rank move in one loop." data-i18n-zh="选择玩法、配置 Agent 阵容、提交成果，然后在同一条循环里看到奖励和段位变化。">Pick a mode, draft your Agent squad, submit a result, then watch rewards and rank move in one loop.</p>
      <a class="cta" href='#league-battle-console' data-i18n-en="Enter via /league" data-i18n-zh="通过 /league 入场">Enter via /league</a>
      <div class="league-hero-actions" aria-label="League quick actions" data-i18n-aria-label-en="League quick actions" data-i18n-aria-label-zh="League 快捷行动">
        <a class="cta secondary" href='#league-playable-modes' data-i18n-en="Choose Mode" data-i18n-zh="选择玩法">Choose Mode</a>
        <a class="cta secondary" href='#league-battle-console' data-i18n-en="Draft Squad" data-i18n-zh="配置阵容">Draft Squad</a>
        <a class="cta secondary" href='#league-progression' data-i18n-en="View Progress" data-i18n-zh="查看成长">View Progress</a>
      </div>
    </aside>
  </header>
  <main>
    <section class="stats">
      <div class="stat"><span data-i18n-en="Players" data-i18n-zh="玩家">Players</span><b>{players}</b></div>
      <div class="stat"><span data-i18n-en="Arenas" data-i18n-zh="赛场">Arenas</span><b>{matches}</b></div>
      <div class="stat"><span data-i18n-en="Battles" data-i18n-zh="战斗">Battles</span><b>{battles}</b></div>
      <div class="stat"><span data-i18n-en="Rewards" data-i18n-zh="奖励">Rewards</span><b>{rewards:.2}</b></div>
      <div class="stat"><span data-i18n-en="Items" data-i18n-zh="道具">Items</span><b>{items}</b></div>
    </section>
    <section id="league-playable-modes">
      <h2 data-i18n-en="Playable Modes" data-i18n-zh="可进入玩法">Playable Modes</h2>
      <div class="grid">{match_cards}</div>
    </section>
    <section id="league-progression" class="panel">
      <h2 data-i18n-en="Progression System" data-i18n-zh="角色成长系统">Progression System</h2>
      <p class="subtitle" data-i18n-en="Schools, skill trees, equipment/tools, skins, multi-Agent capability, experience data, and task-success-based levels." data-i18n-zh="门派系统、技能树、装备/工具、皮肤、多 Agent 能力、经验数据积累和以任务成功数量为核心的等级系统。">Schools, skill trees, equipment/tools, skins, multi-Agent capability, experience data, and task-success-based levels.</p>
      <div class="commands"><code data-i18n-en="{progression_line_en}" data-i18n-zh="{progression_line_zh}">{progression_line_en}</code><code>/progression</code><code>/skills</code><code>/tools</code><code>/skins</code></div>
    </section>
    <section id="league-world-bridge" class="panel">
      <h2>Trillionnium World</h2>
      <p class="subtitle" data-i18n-en="Reality-mirror open world for global players: cities, studios, markets, Agent residents, items, and free actions." data-i18n-zh="面向全球玩家的现实镜像开放世界：城市、工坊、集市、Agent 居民、道具和自由行动。">Reality-mirror open world for global players: cities, studios, markets, Agent residents, items, and free actions.</p>
      <div class="commands"><code>/world</code><code data-i18n-en="/world action Launch an AI Design Studio" data-i18n-zh="/world action 我要开一家 AI 设计工坊">/world action Launch an AI Design Studio</code><code><span data-i18n-en="Items" data-i18n-zh="道具">Items</span> {world_assets}</code><code><span data-i18n-en="Events" data-i18n-zh="事件">Events</span> {world_events}</code></div>
    </section>
    <section id="league-battle-console" class="play">
      <div class="panel">
        <h2 data-i18n-en="Web Battle Console" data-i18n-zh="网页战斗台">Web Battle Console</h2>
        <p class="subtitle">{console_note}</p>
        <form method="post" action="/league/web/action">
          {csrf_input}
          <input type="hidden" name="matrix_user_id" value="@alice:local.dev" />
          <select name="action"><option value="join" data-i18n-en="Join Arena" data-i18n-zh="加入赛场">Join Arena</option><option value="guild" data-i18n-en="Join Guild" data-i18n-zh="加入公会">Join Guild</option><option value="team" data-i18n-en="Join Raid Team" data-i18n-zh="加入团本队伍">Join Raid Team</option><option value="draft" data-i18n-en="Draft Loadout" data-i18n-zh="配置阵容">Draft Loadout</option><option value="raid" data-i18n-en="Contribute Raid" data-i18n-zh="推进团本">Contribute Raid</option><option value="submit" data-i18n-en="Submit Result" data-i18n-zh="提交战果">Submit Result</option></select>
          <input name="match_id" value="daily-dungeon-001" aria-label="match id" data-i18n-aria-label-en="match id" data-i18n-aria-label-zh="赛场 id" />
          <input name="guild_id" value="guild-prompt-forge" aria-label="guild id" data-i18n-aria-label-en="guild id" data-i18n-aria-label-zh="公会 id" />
          <input name="role" value="scout" aria-label="raid role" data-i18n-aria-label-en="raid role" data-i18n-aria-label-zh="团本角色" />
          <input name="heroes" value="oracle_scout forge_builder mirror_auditor courier_closer" aria-label="heroes" />
          <textarea name="body" data-i18n-value-en="Web clear for global beta: deliverable/result, evidence, risk, self-review, next action. Raid options: scout evidence, assign builders, define Boss risk gates." data-i18n-value-zh="网页通关：写清成果、证据、风险、自评和下一步。团本选项：侦察证据、分配建造者、定义 Boss 风险门槛。">Web clear for global beta: deliverable/result, evidence, risk, self-review, next action. Raid options: scout evidence, assign builders, define Boss risk gates.</textarea>
          <button type="submit" data-i18n-en="Play Action" data-i18n-zh="执行行动">Play Action</button>
        </form>
      </div>
      <div class="panel">
        <h2><span data-i18n-en="Battle Timeline" data-i18n-zh="战斗时间线">Battle Timeline</span> · <span data-i18n-en="Replay" data-i18n-zh="回放">Replay</span></h2>
        <p class="subtitle"><span data-i18n-en="Current loadout:" data-i18n-zh="当前阵容：">Current loadout:</span> {loadout_line}</p>
        <p class="subtitle"><span data-i18n-en="Top loot:" data-i18n-zh="最强掉落：">Top loot:</span> {top_loot}</p>
        <ul class="timeline">{timeline}</ul>
      </div>
    </section>
    <section id="league-scoring-rewards" class="panel">
      <h2 data-i18n-en="Scoring & Rewards" data-i18n-zh="评分与奖励">Scoring & Rewards</h2>
      <p class="subtitle" data-i18n-en="{latest_rating_line_en}" data-i18n-zh="{latest_rating_line_zh}">{latest_rating_line_en}</p>
      <div class="reward-formula"><strong data-i18n-en="Reward formula" data-i18n-zh="奖励公式">Reward formula</strong><span data-i18n-en="{reward_formula_en}" data-i18n-zh="{reward_formula_zh}">{reward_formula_en}</span><code>delivery 30% · evidence 24% · risk 18% · next action 16% · polish 12%</code></div>
      <details class="dev-details score-breakdown-drawer"><summary data-i18n-en="Rubric breakdown" data-i18n-zh="展开评分明细">Rubric breakdown</summary><div class="score-grid">{score_breakdown_cards}</div></details>
    </section>
    <section class="panel">
      <h2 data-i18n-en="Guild Halls" data-i18n-zh="公会大厅">Guild Halls</h2>
      <div class="mini-grid">{guild_cards}</div>
    </section>
    <section class="panel">
      <h2 data-i18n-en="Leaderboard" data-i18n-zh="排行榜">Leaderboard</h2>
      <table><thead><tr><th>#</th><th data-i18n-en="Player" data-i18n-zh="玩家">Player</th><th data-i18n-en="Rank" data-i18n-zh="段位">Rank</th><th data-i18n-en="RP" data-i18n-zh="积分">RP</th><th data-i18n-en="Earned" data-i18n-zh="已获奖励">Earned</th></tr></thead><tbody>{leaderboard}</tbody></table>
    </section>
    <section class="panel">
      <h2 data-i18n-en="Playable Commands" data-i18n-zh="可用指令">Playable Commands</h2>
      <div class="commands"><code>/arena</code><code>/join daily-dungeon-001</code><code>/battle daily-dungeon-001 &lt;action&gt;</code><code>/submit daily-dungeon-001 &lt;result&gt;</code><code>/rank</code><code>/profile</code><code>/rewards</code><code>/history</code></div>
    </section>
  </main>
  {language_runtime_script}
</body>
</html>"#,
        players = league.players_by_matrix_user.len(),
        matches = league.matches.len(),
        battles = league.battles.len(),
        rewards = total_rewards,
        items = player_items.len(),
        match_cards = match_cards,
        guild_cards = guild_cards,
        progression_line_en = escape_html_text(&progression_line_en),
        progression_line_zh = escape_html_text(&progression_line_zh),
        latest_rating_line_en = escape_html_text(&latest_rating_line_en),
        latest_rating_line_zh = escape_html_text(&latest_rating_line_zh),
        reward_formula_en = escape_html_text(reward_formula_en),
        reward_formula_zh = escape_html_text(reward_formula_zh),
        score_breakdown_cards = score_breakdown_cards,
        timeline = timeline,
        loadout_line = loadout_line,
        top_loot = escape_html_text(&top_loot),
        leaderboard = leaderboard,
        world_assets = league.world.world_assets.len(),
        world_events = league.world.world_events.len(),
        console_note = console_note_html,
        csrf_input = csrf_input,
        language_runtime_script = trillionnium_language_runtime_script(),
    ))
}

pub(super) async fn post_league_web_session(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<LeagueWebSessionRequest>,
) -> Response {
    let matrix_user_id = match normalize_league_matrix_user(&payload.matrix_user_id) {
        Some(value) => value,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "matrix_user_id is required" })),
            )
                .into_response()
        }
    };
    let Some(secret) = league_web_session_secret(state.config()) else {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "league web session secret is not configured" })),
        )
            .into_response();
    };
    if !matches!(state.config().runtime_profile, RuntimeProfile::LocalDev) {
        let scope = IdentityScope {
            source_kind: "league_web_session",
            user_id: Some(matrix_user_id.clone()),
            room_id: payload.room_id.clone(),
            session_id: payload.session_id.clone(),
            org_id: None,
            account_id: None,
        };
        let fingerprint = format!(
            "league-web-session:{}:{}:{}",
            matrix_user_id,
            payload.room_id.as_deref().unwrap_or_default(),
            payload.session_id.as_deref().unwrap_or_default(),
        );
        if let Err(response) = authorize_user_session(&state, &headers, &scope, &fingerprint) {
            return response;
        }
    }
    let now = Utc::now().timestamp();
    let csrf = payload
        .csrf
        .clone()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| {
            league_web_csrf(secret, &matrix_user_id, payload.room_id.as_deref(), now)
        });
    let claims = LeagueWebSessionClaims {
        version: 1,
        matrix_user_id: matrix_user_id.clone(),
        room_id: payload.room_id.clone(),
        session_id: payload.session_id.clone(),
        csrf,
        issued_at_epoch: now,
        expires_at_epoch: now + state.config().league_web_session_ttl_secs as i64,
    };
    let token = match encode_league_web_session(&claims, secret) {
        Ok(value) => value,
        Err(response) => return response,
    };
    let secure = if matches!(state.config().runtime_profile, RuntimeProfile::LocalDev) {
        ""
    } else {
        "; Secure"
    };
    let cookie = format!(
        "{}={}; Path=/; HttpOnly; SameSite=Lax; Max-Age={}{}",
        state.config().league_web_session_cookie_name,
        token,
        state.config().league_web_session_ttl_secs,
        secure,
    );
    (
        StatusCode::OK,
        [(header::SET_COOKIE, cookie)],
        Json(json!({
            "kind": "league_web_session",
            "league": "trillionnium_league",
            "matrix_user_id": matrix_user_id,
            "room_id": payload.room_id,
            "session_id": payload.session_id,
            "csrf": claims.csrf,
            "expires_at_epoch": claims.expires_at_epoch,
        })),
    )
        .into_response()
}

pub(super) async fn post_league_web_action(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(payload): Form<LeagueWebActionRequest>,
) -> Response {
    let web_session = match authorize_league_web_session(&state, &headers, payload.csrf.as_deref())
    {
        Ok(value) => value,
        Err(response) => {
            if matches!(state.config().runtime_profile, RuntimeProfile::LocalDev)
                && cookie_value(&headers, &state.config().league_web_session_cookie_name).is_none()
            {
                None
            } else {
                return response;
            }
        }
    };

    let matrix_user_id = web_session
        .as_ref()
        .map(|session| session.matrix_user_id.clone())
        .or_else(|| {
            normalize_league_matrix_user(
                payload
                    .matrix_user_id
                    .as_deref()
                    .unwrap_or("@alice:local.dev"),
            )
        })
        .unwrap_or_else(|| "@alice:local.dev".to_string());
    let action = payload.action.trim().to_ascii_lowercase();

    let snapshot = match action.as_str() {
        "join" => {
            let match_id = payload
                .match_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or("daily-dungeon-001")
                .to_string();
            let mut league = state.inner.league_state.lock().await;
            if !league.matches.contains_key(&match_id) {
                return (
                    StatusCode::NOT_FOUND,
                    Json(json!({ "error": "league match not found", "match_id": match_id })),
                )
                    .into_response();
            }
            let player = ensure_league_player(&mut league, &matrix_user_id, None);
            ensure_league_entry(&mut league, &match_id, &matrix_user_id, &player.player_id);
            league.clone()
        }
        "guild" => {
            let guild_id = payload
                .guild_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or("guild-prompt-forge")
                .to_string();
            let mut league = state.inner.league_state.lock().await;
            if !league.guilds.contains_key(&guild_id) {
                return (
                    StatusCode::NOT_FOUND,
                    Json(json!({ "error": "league guild not found", "guild_id": guild_id })),
                )
                    .into_response();
            }
            let player = ensure_league_player(&mut league, &matrix_user_id, None);
            league.guild_memberships.insert(
                matrix_user_id.clone(),
                LeagueGuildMembership {
                    guild_id,
                    player_id: player.player_id.clone(),
                    matrix_user_id: matrix_user_id.clone(),
                    role: "member".to_string(),
                    joined_at_epoch: Utc::now().timestamp(),
                },
            );
            league.clone()
        }
        "draft" => {
            let heroes = normalize_hero_draft(
                payload
                    .heroes
                    .as_deref()
                    .unwrap_or("oracle_scout forge_builder mirror_auditor courier_closer")
                    .split_whitespace()
                    .map(ToString::to_string)
                    .collect(),
            );
            if heroes.len() < 3 {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(json!({
                        "error": "draft requires at least 3 unique heroes",
                        "examples": "oracle_scout forge_builder mirror_auditor courier_closer"
                    })),
                )
                    .into_response();
            }
            let mut league = state.inner.league_state.lock().await;
            ensure_league_player(&mut league, &matrix_user_id, None);
            league
                .player_loadouts
                .insert(matrix_user_id.clone(), heroes);
            league.clone()
        }
        "submit" => {
            let match_id = payload
                .match_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or("daily-dungeon-001")
                .to_string();
            let body = match validate_text_payload(
                payload
                    .body
                    .as_deref()
                    .unwrap_or("Web action: evidence, risk, deliverable, next step."),
                state.config().max_text_chars,
            ) {
                Ok(value) => value,
                Err(response) => return response,
            };
            let league_match_mode = {
                let league = state.inner.league_state.lock().await;
                let Some(league_match) = league.matches.get(&match_id).cloned() else {
                    return (
                        StatusCode::NOT_FOUND,
                        Json(json!({ "error": "league match not found", "match_id": match_id })),
                    )
                        .into_response();
                };
                league_match.mode
            };
            let judgement =
                judge_league_submission_with_pipeline(&state, &body, &league_match_mode).await;
            let (submission, mut reward) = {
                let mut league = state.inner.league_state.lock().await;
                if !league.matches.contains_key(&match_id) {
                    return (
                        StatusCode::NOT_FOUND,
                        Json(json!({ "error": "league match not found", "match_id": match_id })),
                    )
                        .into_response();
                }
                let player = ensure_league_player(&mut league, &matrix_user_id, None);
                let entry =
                    ensure_league_entry(&mut league, &match_id, &matrix_user_id, &player.player_id);
                let score = judgement.score;
                let grade = judgement.grade.clone();
                let reward_amount = judgement.reward_amount;
                let now = Utc::now().timestamp();
                let submission_id = league_hash_id(
                    "submission",
                    &format!("web:{}:{}:{}:{}", match_id, entry.entry_id, now, body),
                );
                let submission = LeagueSubmission {
                    submission_id: submission_id.clone(),
                    match_id: match_id.clone(),
                    entry_id: entry.entry_id.clone(),
                    player_id: player.player_id.clone(),
                    matrix_user_id: matrix_user_id.clone(),
                    task_id: None,
                    body: body.clone(),
                    score,
                    grade: grade.clone(),
                    reward_amount,
                    judge_status: Some(judgement.judge_status.clone()),
                    payout_status: Some(judgement.payout_status.clone()),
                    anti_cheat_flags: judgement.anti_cheat_flags.clone(),
                    score_events: judgement.score_events.clone(),
                    created_at_epoch: now,
                };
                let reward = LeagueReward {
                    reward_id: league_hash_id("reward", &submission_id),
                    match_id: match_id.clone(),
                    entry_id: entry.entry_id.clone(),
                    player_id: player.player_id.clone(),
                    matrix_user_id: matrix_user_id.clone(),
                    amount: reward_amount,
                    currency_unit: "credit".to_string(),
                    reason: format!("league_web_submission_score_{score:.1}_{grade}"),
                    ledger_status: Some("pending".to_string()),
                    ledger_account_id: None,
                    ledger_entry_id: None,
                    ledger_balance_after: None,
                    ledger_error: None,
                    review_status: if judgement.payout_status == "review_hold" {
                        Some("pending_review".to_string())
                    } else {
                        None
                    },
                    reviewed_by: None,
                    review_note: None,
                    reviewed_at_epoch: None,
                    created_at_epoch: now,
                };
                league
                    .submissions
                    .insert(submission_id.clone(), submission.clone());
                league.rewards.push(reward.clone());
                (submission, reward)
            };
            let submit_payload = LeagueSubmitRequest {
                matrix_user_id: matrix_user_id.clone(),
                room_id: Some("!web-local:local.dev".to_string()),
                task_id: None,
                body: submission.body.clone(),
            };
            let settlement = settle_league_reward_with_ledger(
                &state,
                &submit_payload,
                &matrix_user_id,
                &submission,
                &reward,
            )
            .await;
            reward.ledger_status = Some(settlement.status);
            reward.ledger_account_id = settlement.account_id;
            reward.ledger_entry_id = settlement.entry_id;
            reward.ledger_balance_after = settlement.balance_after;
            reward.ledger_error = settlement.error;

            let settlement_completed = league_reward_ledger_released(&reward);
            let mut league = state.inner.league_state.lock().await;
            if let Some(stored_reward) = league
                .rewards
                .iter_mut()
                .find(|stored| stored.reward_id == reward.reward_id)
            {
                *stored_reward = reward.clone();
            }
            if settlement_completed {
                release_league_submission_progression(&mut league, &submission, &reward);
            }
            league.clone()
        }
        "raid" => {
            let match_id = payload
                .match_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or("guild-raid-001")
                .to_string();
            let body = match validate_text_payload(
                payload.body.as_deref().unwrap_or(
                    "Raid contribution: scout evidence, assign builder, define risk gate.",
                ),
                state.config().max_text_chars,
            ) {
                Ok(value) => value,
                Err(response) => return response,
            };
            let request = LeagueRaidContributionRequest {
                matrix_user_id: matrix_user_id.clone(),
                room_id: Some("!web-local:local.dev".to_string()),
                role: Some("web_raider".to_string()),
                body,
            };
            let snapshot = match record_league_raid_contribution(&state, &match_id, request).await {
                Ok((snapshot, _contribution, _progress)) => snapshot,
                Err(response) => return response,
            };
            snapshot
        }
        "team" => {
            let match_id = payload
                .match_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or("guild-raid-001")
                .to_string();
            let hero_id = payload
                .heroes
                .as_deref()
                .and_then(|heroes| heroes.split_whitespace().next())
                .map(ToString::to_string)
                .or_else(|| Some("oracle_scout".to_string()));
            let request = LeagueRaidRosterRequest {
                matrix_user_id: matrix_user_id.clone(),
                room_id: Some("!web-local:local.dev".to_string()),
                role: payload.role.clone().or_else(|| Some("scout".to_string())),
                hero_id,
            };
            let snapshot = match record_league_raid_roster_slot(&state, &match_id, request).await {
                Ok((snapshot, _slot, _roster)) => snapshot,
                Err(response) => return response,
            };
            snapshot
        }
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "error": "unsupported league web action",
                    "allowed": ["join", "guild", "draft", "submit", "raid", "team"]
                })),
            )
                .into_response()
        }
    };

    if let Err(response) = persist_league_state(&state, &snapshot).await {
        return response;
    }

    Redirect::to("/league?played=1").into_response()
}

pub(super) async fn get_league_raids(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }
    let league = state.inner.league_state.lock().await;
    let raids: Vec<LeagueMatch> = league
        .matches
        .values()
        .filter(|league_match| league_match.mode == "guild_raid")
        .cloned()
        .collect();
    let progress: Vec<Value> = raids
        .iter()
        .map(|raid| league_raid_progress(&league, &raid.match_id))
        .collect();
    (
        StatusCode::OK,
        Json(json!({
            "kind": "league_raids",
            "league": "trillionnium_league",
            "raids": raids,
            "progress": progress,
            "guilds": league_guild_standings(&league),
        })),
    )
        .into_response()
}

pub(super) async fn contribute_league_raid(
    Path(match_id): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<LeagueRaidContributionRequest>,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }
    let (snapshot, contribution, progress) =
        match record_league_raid_contribution(&state, &match_id, payload).await {
            Ok(value) => value,
            Err(response) => return response,
        };
    if let Err(response) = persist_league_state(&state, &snapshot).await {
        return response;
    }
    (
        StatusCode::OK,
        Json(json!({
            "kind": "league_raid_contribution",
            "league": "trillionnium_league",
            "contribution": contribution,
            "progress": progress,
        })),
    )
        .into_response()
}

pub(super) async fn get_league_raid_roster(
    Path(match_id): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }
    let league = state.inner.league_state.lock().await;
    if league
        .matches
        .get(&match_id)
        .is_none_or(|league_match| league_match.mode != "guild_raid")
    {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "league raid not found", "match_id": match_id })),
        )
            .into_response();
    }
    let roster = league_raid_roster_summary(&league, &match_id);
    (
        StatusCode::OK,
        Json(json!({
            "kind": "league_raid_roster",
            "league": "trillionnium_league",
            "match_id": match_id,
            "roster": roster,
        })),
    )
        .into_response()
}

pub(super) async fn join_league_raid_roster(
    Path(match_id): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<LeagueRaidRosterRequest>,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }
    let (snapshot, slot, roster) =
        match record_league_raid_roster_slot(&state, &match_id, payload).await {
            Ok(value) => value,
            Err(response) => return response,
        };
    if let Err(response) = persist_league_state(&state, &snapshot).await {
        return response;
    }
    (
        StatusCode::OK,
        Json(json!({
            "kind": "league_raid_roster_joined",
            "league": "trillionnium_league",
            "slot": slot,
            "roster": roster,
        })),
    )
        .into_response()
}

fn league_submission_for_reward<'a>(
    league: &'a LeagueState,
    reward: &LeagueReward,
) -> Option<&'a LeagueSubmission> {
    league
        .submissions
        .values()
        .find(|submission| league_hash_id("reward", &submission.submission_id) == reward.reward_id)
}

fn league_reward_already_released(reward: &LeagueReward) -> bool {
    matches!(
        reward.ledger_status.as_deref(),
        Some("settled") | Some("duplicate")
    ) || reward.review_status.as_deref() == Some("approved")
}

fn league_reward_ledger_released(reward: &LeagueReward) -> bool {
    matches!(
        reward.ledger_status.as_deref(),
        Some("settled") | Some("duplicate")
    )
}

fn release_league_submission_progression(
    league: &mut LeagueState,
    submission: &LeagueSubmission,
    reward: &LeagueReward,
) {
    if let Some(player) = league
        .players_by_matrix_user
        .get_mut(&submission.matrix_user_id)
    {
        player.submissions += 1;
        player.xp += submission.score.round() as i64;
        player.reputation += (submission.score / 10.0).round() as i64;
        player.rating += ((submission.score - 50.0) / 2.0).round() as i64;
        if submission.score >= 80.0 {
            player.wins += 1;
        }
        player.earned_credits += reward.amount;
    }
    if let Some(entry) = league.entries.get_mut(&league_entry_key(
        &submission.match_id,
        &submission.matrix_user_id,
    )) {
        entry.submissions += 1;
        entry.best_score = entry.best_score.max(submission.score);
        entry.rewards_earned += reward.amount;
    }
    if !league
        .inventory_items
        .iter()
        .any(|item| item.source_submission_id == submission.submission_id)
    {
        league
            .inventory_items
            .push(league_item_for_submission(submission));
    }
}

pub(super) async fn get_league_held_reviews(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }
    let league = state.inner.league_state.lock().await;
    let held: Vec<Value> = league
        .rewards
        .iter()
        .filter(|reward| {
            reward
                .ledger_status
                .as_deref()
                .is_some_and(|status| status == "held_review")
                || reward
                    .review_status
                    .as_deref()
                    .is_some_and(|status| status == "pending_review" || status == "approval_failed")
        })
        .map(|reward| {
            json!({
                "reward": reward,
                "submission": league_submission_for_reward(&league, reward),
            })
        })
        .collect();
    (
        StatusCode::OK,
        Json(json!({
            "kind": "league_review_queue",
            "league": "trillionnium_league",
            "held_count": held.len(),
            "held": held,
        })),
    )
        .into_response()
}

pub(super) async fn approve_league_review(
    Path(reward_id): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<LeagueReviewRequest>,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }
    let reviewer_id = payload
        .reviewer_id
        .clone()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "local-reviewer".to_string());
    let review_note = payload
        .note
        .clone()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let (reward, submission) = {
        let league = state.inner.league_state.lock().await;
        let Some(reward) = league
            .rewards
            .iter()
            .find(|reward| reward.reward_id == reward_id)
            .cloned()
        else {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "league reward not found", "reward_id": reward_id })),
            )
                .into_response();
        };
        if league_reward_already_released(&reward) {
            return (
                StatusCode::CONFLICT,
                Json(json!({ "error": "league reward already released", "reward_id": reward_id })),
            )
                .into_response();
        }
        let Some(submission) = league_submission_for_reward(&league, &reward).cloned() else {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "league submission for reward not found", "reward_id": reward_id })),
            )
                .into_response();
        };
        (reward, submission)
    };
    let mut settlement_submission = submission.clone();
    settlement_submission.payout_status = Some("approved_release".to_string());
    let submit_payload = LeagueSubmitRequest {
        matrix_user_id: reward.matrix_user_id.clone(),
        room_id: payload
            .room_id
            .clone()
            .or_else(|| Some("!web-local:local.dev".to_string())),
        task_id: submission.task_id.clone(),
        body: submission.body.clone(),
    };
    let settlement = settle_league_reward_with_ledger(
        &state,
        &submit_payload,
        &reward.matrix_user_id,
        &settlement_submission,
        &reward,
    )
    .await;
    let now = Utc::now().timestamp();
    let released = matches!(settlement.status.as_str(), "settled" | "duplicate");
    let snapshot = {
        let mut league = state.inner.league_state.lock().await;
        if let Some(stored_submission) = league.submissions.get_mut(&submission.submission_id) {
            stored_submission.payout_status = Some(
                if released {
                    "approved_release"
                } else {
                    "review_hold"
                }
                .to_string(),
            );
            stored_submission.score_events.push(LeagueScoreEvent {
                dimension: if released {
                    "human_review_release"
                } else {
                    "human_review_release_failed"
                }
                .to_string(),
                score: stored_submission.score,
                weight: 0.0,
                judge_kind: "review_admin_v1".to_string(),
                evidence: json!({"reviewer_id": reviewer_id.clone(), "note": review_note.clone(), "ledger_status": settlement.status.clone()}),
            });
        }
        if released {
            let mut released_reward = reward.clone();
            released_reward.ledger_status = Some(settlement.status.clone());
            release_league_submission_progression(&mut league, &submission, &released_reward);
        }
        if let Some(stored_reward) = league
            .rewards
            .iter_mut()
            .find(|stored| stored.reward_id == reward.reward_id)
        {
            stored_reward.ledger_status = Some(settlement.status.clone());
            stored_reward.ledger_account_id = settlement.account_id.clone();
            stored_reward.ledger_entry_id = settlement.entry_id.clone();
            stored_reward.ledger_balance_after = settlement.balance_after;
            stored_reward.ledger_error = settlement.error.clone();
            stored_reward.review_status = Some(
                if released {
                    "approved"
                } else {
                    "approval_failed"
                }
                .to_string(),
            );
            stored_reward.reviewed_by = Some(reviewer_id.clone());
            stored_reward.review_note = review_note.clone();
            stored_reward.reviewed_at_epoch = Some(now);
        }
        league.clone()
    };
    if let Err(response) = persist_league_state(&state, &snapshot).await {
        return response;
    }
    (
        StatusCode::OK,
        Json(json!({
            "kind": "league_review_approved",
            "league": "trillionnium_league",
            "reward_id": reward.reward_id,
            "review_status": if released { "approved" } else { "approval_failed" },
            "ledger_status": settlement.status,
            "ledger_entry_id": settlement.entry_id,
            "ledger_error": settlement.error,
        })),
    )
        .into_response()
}

pub(super) async fn reject_league_review(
    Path(reward_id): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<LeagueReviewRequest>,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }
    let reviewer_id = payload
        .reviewer_id
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "local-reviewer".to_string());
    let review_note = payload
        .note
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let now = Utc::now().timestamp();
    let snapshot = {
        let mut league = state.inner.league_state.lock().await;
        let Some(reward_index) = league
            .rewards
            .iter()
            .position(|reward| reward.reward_id == reward_id)
        else {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "league reward not found", "reward_id": reward_id })),
            )
                .into_response();
        };
        let reward = league.rewards[reward_index].clone();
        if league_reward_already_released(&reward) {
            return (
                StatusCode::CONFLICT,
                Json(json!({ "error": "league reward already released", "reward_id": reward_id })),
            )
                .into_response();
        }
        let submission_id = league_submission_for_reward(&league, &reward)
            .map(|submission| submission.submission_id.clone());
        if let Some(submission_id) = submission_id {
            if let Some(submission) = league.submissions.get_mut(&submission_id) {
                submission.payout_status = Some("rejected".to_string());
                submission.score_events.push(LeagueScoreEvent {
                    dimension: "human_review_reject".to_string(),
                    score: submission.score,
                    weight: 0.0,
                    judge_kind: "review_admin_v1".to_string(),
                    evidence: json!({"reviewer_id": reviewer_id.clone(), "note": review_note.clone()}),
                });
            }
        }
        let reward = &mut league.rewards[reward_index];
        reward.ledger_status = Some("rejected".to_string());
        reward.review_status = Some("rejected".to_string());
        reward.reviewed_by = Some(reviewer_id);
        reward.review_note = review_note;
        reward.reviewed_at_epoch = Some(now);
        league.clone()
    };
    if let Err(response) = persist_league_state(&state, &snapshot).await {
        return response;
    }
    (
        StatusCode::OK,
        Json(json!({
            "kind": "league_review_rejected",
            "league": "trillionnium_league",
            "reward_id": reward_id,
            "review_status": "rejected",
            "ledger_status": "rejected",
        })),
    )
        .into_response()
}

pub(super) async fn get_league_matches(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }

    let league = state.inner.league_state.lock().await;
    let mut matches: Vec<LeagueMatch> = league.matches.values().cloned().collect();
    matches.sort_by(|left, right| left.match_id.cmp(&right.match_id));
    (
        StatusCode::OK,
        Json(json!({
            "kind": "league_matches",
            "league": "trillionnium_league",
            "matches": matches,
        })),
    )
        .into_response()
}

pub(super) async fn get_league_state_snapshot(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }
    let league = state.inner.league_state.lock().await;
    let repository_snapshot = match LeagueStateRepositorySnapshot::from_league(&league) {
        Ok(value) => value,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("failed to build league state repository snapshot: {err}") })),
            )
                .into_response()
        }
    };
    (
        StatusCode::OK,
        Json(repository_snapshot.endpoint_json(
            &league,
            state.config().league_state_path.is_some(),
            state.config().league_sql_snapshot_path.is_some(),
            state.config().league_normalized_dual_write_enabled,
            state.config().league_normalized_read_switch_enabled,
            state.config().league_normalized_database_url.is_some(),
        )),
    )
        .into_response()
}

pub(super) async fn get_league_guilds(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }
    let league = state.inner.league_state.lock().await;
    let mut guilds: Vec<LeagueGuild> = league.guilds.values().cloned().collect();
    guilds.sort_by(|left, right| {
        right
            .rating
            .cmp(&left.rating)
            .then_with(|| left.guild_id.cmp(&right.guild_id))
    });
    let standings = league_guild_standings(&league);
    (
        StatusCode::OK,
        Json(json!({
            "kind": "league_guilds",
            "league": "trillionnium_league",
            "guilds": guilds,
            "standings": standings,
        })),
    )
        .into_response()
}

pub(super) async fn join_league_guild(
    Path(guild_id): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<LeagueJoinRequest>,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }
    let matrix_user_id = match normalize_league_matrix_user(&payload.matrix_user_id) {
        Some(value) => value,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "matrix_user_id is required" })),
            )
                .into_response()
        }
    };
    let (guild, player, membership, snapshot) = {
        let mut league = state.inner.league_state.lock().await;
        let Some(guild) = league.guilds.get(&guild_id).cloned() else {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "league guild not found", "guild_id": guild_id })),
            )
                .into_response();
        };
        let player = ensure_league_player(
            &mut league,
            &matrix_user_id,
            payload.display_name.as_deref(),
        );
        let membership = LeagueGuildMembership {
            guild_id: guild_id.clone(),
            player_id: player.player_id.clone(),
            matrix_user_id: matrix_user_id.clone(),
            role: "member".to_string(),
            joined_at_epoch: Utc::now().timestamp(),
        };
        league
            .guild_memberships
            .insert(matrix_user_id.clone(), membership.clone());
        let snapshot = league.clone();
        (guild, player, membership, snapshot)
    };
    if let Err(response) = persist_league_state(&state, &snapshot).await {
        return response;
    }
    (
        StatusCode::OK,
        Json(json!({
            "kind": "league_guild_joined",
            "league": "trillionnium_league",
            "guild": guild,
            "player": player,
            "membership": membership,
            "room_id": payload.room_id,
        })),
    )
        .into_response()
}

pub(super) async fn join_league_match(
    Path(match_id): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<LeagueJoinRequest>,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }
    let matrix_user_id = match normalize_league_matrix_user(&payload.matrix_user_id) {
        Some(value) => value,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "matrix_user_id is required" })),
            )
                .into_response()
        }
    };

    let (league_match, player, entry, snapshot) = {
        let mut league = state.inner.league_state.lock().await;
        let Some(league_match) = league.matches.get(&match_id).cloned() else {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "league match not found", "match_id": match_id })),
            )
                .into_response();
        };
        let player = ensure_league_player(
            &mut league,
            &matrix_user_id,
            payload.display_name.as_deref(),
        );
        let entry = ensure_league_entry(&mut league, &match_id, &matrix_user_id, &player.player_id);
        let snapshot = league.clone();
        (league_match, player, entry, snapshot)
    };
    if let Err(response) = persist_league_state(&state, &snapshot).await {
        return response;
    }
    (
        StatusCode::OK,
        Json(json!({
            "kind": "league_joined",
            "league": "trillionnium_league",
            "match": league_match,
            "player": player,
            "entry": entry,
            "room_id": payload.room_id,
        })),
    )
        .into_response()
}

pub(super) async fn create_league_battle(
    Path(match_id): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<LeagueBattleRequest>,
) -> Response {
    state.inner.metrics.inc_task_create_requests();
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }
    let matrix_user_id = match normalize_league_matrix_user(&payload.matrix_user_id) {
        Some(value) => value,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "matrix_user_id is required" })),
            )
                .into_response()
        }
    };

    let (league_match, player, entry) = {
        let mut league = state.inner.league_state.lock().await;
        let Some(league_match) = league.matches.get(&match_id).cloned() else {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "league match not found", "match_id": match_id })),
            )
                .into_response();
        };
        let player = ensure_league_player(&mut league, &matrix_user_id, None);
        let mut entry =
            ensure_league_entry(&mut league, &match_id, &matrix_user_id, &player.player_id);
        entry.battles_started += 1;
        league
            .entries
            .insert(league_entry_key(&match_id, &matrix_user_id), entry.clone());
        (league_match, player, entry)
    };

    let metadata = merge_league_battle_metadata(payload.metadata, &match_id, &entry.entry_id);
    let matrix_payload = MatrixMessageRequest {
        matrix_user_id: matrix_user_id.clone(),
        room_id: payload.room_id,
        session_id: None,
        org_id: None,
        message: payload.message,
        capability_id: payload.capability_id,
        account_id: payload.account_id,
        event_id: payload.event_id,
        idempotency_key: None,
        metadata: Some(metadata),
    };

    let resolved_identity = match resolve_matrix_identity(&state, &matrix_payload).await {
        Ok(identity) => identity,
        Err(response) => return response,
    };
    let request_fingerprint = build_matrix_request_fingerprint(&matrix_payload);
    let authorized_session = match authorize_user_session(
        &state,
        &headers,
        &resolved_identity.scope,
        request_fingerprint.as_str(),
    ) {
        Ok(session) => session,
        Err(response) => return response,
    };
    let prompt = match validate_text_payload(&matrix_payload.message, state.config().max_text_chars)
    {
        Ok(prompt) => prompt,
        Err(response) => return response,
    };
    let resolved_account_id = resolved_identity.scope.account_id.clone();
    let source = json!({
        "kind": "league_battle",
        "league": "trillionnium_league",
        "match_id": match_id,
        "entry_id": entry.entry_id,
        "player_id": player.player_id,
        "identity_scope": resolved_identity.scope,
        "identity_resolution": resolved_identity.resolution,
        "matrix_user_id": matrix_user_id,
        "room_id": matrix_payload.room_id,
        "event_id": matrix_payload.event_id,
        "metadata": matrix_payload.metadata,
        "session_auth": authorized_session,
    });

    let task = match forward_to_cex_task(
        state.clone(),
        matrix_payload.capability_id,
        resolved_account_id,
        source,
        prompt,
    )
    .await
    {
        Ok(response) => response,
        Err(response) => return response,
    };

    let (player, entry, battle, snapshot) = {
        let mut league = state.inner.league_state.lock().await;
        let mut player = ensure_league_player(&mut league, &matrix_user_id, None);
        player.battles += 1;
        let mut entry =
            ensure_league_entry(&mut league, &match_id, &matrix_user_id, &player.player_id);
        let task_id = task.task_id.clone();
        let battle_key = league_hash_id(
            "battle",
            &format!("{}:{}:{}", match_id, entry.entry_id, task_id),
        );
        let battle = LeagueBattle {
            battle_id: battle_key.clone(),
            match_id: match_id.clone(),
            entry_id: entry.entry_id.clone(),
            player_id: player.player_id.clone(),
            matrix_user_id: matrix_user_id.clone(),
            task_id,
            prompt: task
                .request
                .as_ref()
                .and_then(|value| value.get("prompt"))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            status: "created".to_string(),
            created_at_epoch: Utc::now().timestamp(),
        };
        entry.battles_started = entry.battles_started.max(1);
        league
            .players_by_matrix_user
            .insert(matrix_user_id.clone(), player.clone());
        league
            .entries
            .insert(league_entry_key(&match_id, &matrix_user_id), entry.clone());
        league.battles.insert(battle_key, battle.clone());
        let snapshot = league.clone();
        (player, entry, battle, snapshot)
    };
    if let Err(response) = persist_league_state(&state, &snapshot).await {
        return response;
    }

    (
        StatusCode::ACCEPTED,
        Json(json!({
            "kind": "league_battle",
            "league": "trillionnium_league",
            "match": league_match,
            "player": player,
            "entry": entry,
            "battle": battle,
            "task": task,
        })),
    )
        .into_response()
}

pub(super) async fn submit_league_match(
    Path(match_id): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<LeagueSubmitRequest>,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }
    let matrix_user_id = match normalize_league_matrix_user(&payload.matrix_user_id) {
        Some(value) => value,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "matrix_user_id is required" })),
            )
                .into_response()
        }
    };
    let body = match validate_text_payload(&payload.body, state.config().max_text_chars) {
        Ok(value) => value,
        Err(response) => return response,
    };

    let league_match = {
        let league = state.inner.league_state.lock().await;
        let Some(league_match) = league.matches.get(&match_id).cloned() else {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "league match not found", "match_id": match_id })),
            )
                .into_response();
        };
        league_match
    };
    let judgement = judge_league_submission_with_pipeline(&state, &body, &league_match.mode).await;

    let (player, entry, submission, reward, _snapshot) = {
        let mut league = state.inner.league_state.lock().await;
        if !league.matches.contains_key(&match_id) {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "league match not found", "match_id": match_id })),
            )
                .into_response();
        }
        let player = ensure_league_player(&mut league, &matrix_user_id, None);
        let entry = ensure_league_entry(&mut league, &match_id, &matrix_user_id, &player.player_id);
        let score = judgement.score;
        let grade = judgement.grade.clone();
        let reward_amount = judgement.reward_amount;
        let now = Utc::now().timestamp();
        let submission_id = league_hash_id(
            "submission",
            &format!("{}:{}:{}:{}", match_id, entry.entry_id, now, body),
        );
        let submission = LeagueSubmission {
            submission_id: submission_id.clone(),
            match_id: match_id.clone(),
            entry_id: entry.entry_id.clone(),
            player_id: player.player_id.clone(),
            matrix_user_id: matrix_user_id.clone(),
            task_id: payload.task_id.clone(),
            body: body.clone(),
            score,
            grade: grade.clone(),
            reward_amount,
            judge_status: Some(judgement.judge_status.clone()),
            payout_status: Some(judgement.payout_status.clone()),
            anti_cheat_flags: judgement.anti_cheat_flags.clone(),
            score_events: judgement.score_events.clone(),
            created_at_epoch: now,
        };
        let reward = LeagueReward {
            reward_id: league_hash_id("reward", &submission_id),
            match_id: match_id.clone(),
            entry_id: entry.entry_id.clone(),
            player_id: player.player_id.clone(),
            matrix_user_id: matrix_user_id.clone(),
            amount: reward_amount,
            currency_unit: "credit".to_string(),
            reason: format!("league_submission_score_{score:.1}_{grade}"),
            ledger_status: Some("pending".to_string()),
            ledger_account_id: None,
            ledger_entry_id: None,
            ledger_balance_after: None,
            ledger_error: None,
            review_status: if judgement.payout_status == "review_hold" {
                Some("pending_review".to_string())
            } else {
                None
            },
            reviewed_by: None,
            review_note: None,
            reviewed_at_epoch: None,
            created_at_epoch: now,
        };
        league.submissions.insert(submission_id, submission.clone());
        league.rewards.push(reward.clone());
        let snapshot = league.clone();
        (player, entry, submission, reward, snapshot)
    };

    let mut reward = reward;
    let settlement =
        settle_league_reward_with_ledger(&state, &payload, &matrix_user_id, &submission, &reward)
            .await;
    reward.ledger_status = Some(settlement.status);
    reward.ledger_account_id = settlement.account_id;
    reward.ledger_entry_id = settlement.entry_id;
    reward.ledger_balance_after = settlement.balance_after;
    reward.ledger_error = settlement.error;

    let settlement_completed = league_reward_ledger_released(&reward);
    let (response_player, response_entry, snapshot) = {
        let mut league = state.inner.league_state.lock().await;
        if let Some(stored_reward) = league
            .rewards
            .iter_mut()
            .find(|stored| stored.reward_id == reward.reward_id)
        {
            *stored_reward = reward.clone();
        }
        if settlement_completed {
            release_league_submission_progression(&mut league, &submission, &reward);
        }
        let response_player = league
            .players_by_matrix_user
            .get(&matrix_user_id)
            .cloned()
            .unwrap_or_else(|| player.clone());
        let response_entry = league
            .entries
            .get(&league_entry_key(&match_id, &matrix_user_id))
            .cloned()
            .unwrap_or_else(|| entry.clone());
        (response_player, response_entry, league.clone())
    };

    if let Err(response) = persist_league_state(&state, &snapshot).await {
        return response;
    }

    (
        StatusCode::OK,
        Json(json!({
            "kind": "league_submission",
            "league": "trillionnium_league",
            "match": league_match,
            "player": response_player,
            "entry": response_entry,
            "submission": submission,
            "reward": reward,
            "room_id": payload.room_id,
        })),
    )
        .into_response()
}

#[derive(Debug, Clone, Default)]
pub(super) struct LeagueLedgerSettlement {
    pub(super) status: String,
    pub(super) account_id: Option<String>,
    pub(super) entry_id: Option<String>,
    pub(super) balance_after: Option<f64>,
    pub(super) error: Option<String>,
}

pub(super) async fn settle_league_reward_with_ledger(
    state: &AppState,
    payload: &LeagueSubmitRequest,
    matrix_user_id: &str,
    submission: &LeagueSubmission,
    reward: &LeagueReward,
) -> LeagueLedgerSettlement {
    if reward.amount <= 0.0 {
        return LeagueLedgerSettlement {
            status: "skipped_zero_reward".to_string(),
            ..Default::default()
        };
    }
    let payout_status = submission.payout_status.as_deref().unwrap_or("eligible");
    let approved_release = payout_status == "approved_release";
    if (!submission.anti_cheat_flags.is_empty() || payout_status != "eligible") && !approved_release
    {
        return LeagueLedgerSettlement {
            status: "held_review".to_string(),
            error: Some(format!(
                "payout held by review gate: status={payout_status} flags={}",
                submission.anti_cheat_flags.join(",")
            )),
            ..Default::default()
        };
    }

    let Some(room_id) = payload
        .room_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return LeagueLedgerSettlement {
            status: "skipped_missing_room".to_string(),
            ..Default::default()
        };
    };

    let matrix_payload = MatrixMessageRequest {
        matrix_user_id: matrix_user_id.to_string(),
        room_id: room_id.to_string(),
        session_id: None,
        org_id: None,
        message: "league reward settlement".to_string(),
        capability_id: None,
        account_id: None,
        event_id: None,
        idempotency_key: None,
        metadata: None,
    };
    let resolved_identity = match resolve_matrix_identity(state, &matrix_payload).await {
        Ok(identity) => identity,
        Err(_) => {
            return LeagueLedgerSettlement {
                status: "failed_identity".to_string(),
                error: Some(
                    "matrix identity could not be resolved for reward settlement".to_string(),
                ),
                ..Default::default()
            }
        }
    };

    let Some(account_id) = resolved_identity.scope.account_id.clone() else {
        return LeagueLedgerSettlement {
            status: "skipped_missing_account".to_string(),
            error: Some("matrix identity did not resolve a ledger account_id".to_string()),
            ..Default::default()
        };
    };
    let Some(ledger_admin_token) = state.config().ledger_admin_token.clone() else {
        return LeagueLedgerSettlement {
            status: "skipped_missing_ledger_token".to_string(),
            account_id: Some(account_id),
            error: Some("consumer-entry ledger admin token is not configured".to_string()),
            ..Default::default()
        };
    };

    let url = format!(
        "{}/v1/ledger/grant",
        state.config().ledger_base_url.trim_end_matches('/')
    );
    let mut body = json!({
        "account_id": account_id,
        "amount": reward.amount,
        "idempotency_key": format!("league_reward:{}", reward.reward_id),
    });
    if let Some(task_id) = submission
        .task_id
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        body["reference_id"] = json!(task_id);
    }

    let response = match state
        .inner
        .http
        .post(url)
        .header("x-admin-token", ledger_admin_token)
        .json(&body)
        .send()
        .await
    {
        Ok(response) => response,
        Err(err) => {
            return LeagueLedgerSettlement {
                status: "failed_network".to_string(),
                account_id: body
                    .get("account_id")
                    .and_then(Value::as_str)
                    .map(ToString::to_string),
                error: Some(format!("failed to reach ledger-service: {err}")),
                ..Default::default()
            }
        }
    };

    let status = response.status();
    let value = match response.json::<Value>().await {
        Ok(value) => value,
        Err(err) => {
            return LeagueLedgerSettlement {
                status: "failed_bad_response".to_string(),
                account_id: body
                    .get("account_id")
                    .and_then(Value::as_str)
                    .map(ToString::to_string),
                error: Some(format!("ledger-service returned non-json response: {err}")),
                ..Default::default()
            }
        }
    };

    if !status.is_success() {
        let error = value
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("ledger grant failed");
        return LeagueLedgerSettlement {
            status: if status.as_u16() == 409 {
                "duplicate".to_string()
            } else {
                "failed_ledger".to_string()
            },
            account_id: body
                .get("account_id")
                .and_then(Value::as_str)
                .map(ToString::to_string),
            error: Some(format!("{}: {error}", status.as_u16())),
            ..Default::default()
        };
    }

    LeagueLedgerSettlement {
        status: "settled".to_string(),
        account_id: value
            .get("account")
            .and_then(|account| account.get("account_id"))
            .and_then(Value::as_str)
            .or_else(|| body.get("account_id").and_then(Value::as_str))
            .map(ToString::to_string),
        entry_id: value
            .get("entry")
            .and_then(|entry| entry.get("entry_id"))
            .and_then(Value::as_str)
            .map(ToString::to_string),
        balance_after: value
            .get("account")
            .and_then(|account| account.get("balance"))
            .and_then(Value::as_f64),
        error: None,
    }
}

pub(super) async fn get_league_rankings(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }
    let league = state.inner.league_state.lock().await;
    let mut players: Vec<LeaguePlayer> = league.players_by_matrix_user.values().cloned().collect();
    players.sort_by(|left, right| {
        right
            .rating
            .cmp(&left.rating)
            .then_with(|| right.xp.cmp(&left.xp))
            .then_with(|| left.matrix_user_id.cmp(&right.matrix_user_id))
    });
    let guild_standings = league_guild_standings(&league);
    (
        StatusCode::OK,
        Json(json!({
            "kind": "league_rankings",
            "league": "trillionnium_league",
            "season": "preseason-zero",
            "players": players,
            "guilds": guild_standings,
        })),
    )
        .into_response()
}

pub(super) async fn get_league_player_profile(
    Path(matrix_user_id): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }
    let mut league = state.inner.league_state.lock().await;
    let Some(matrix_user_id) = normalize_league_matrix_user(&matrix_user_id) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "matrix_user_id is required" })),
        )
            .into_response();
    };
    let player = ensure_league_player(&mut league, &matrix_user_id, None);
    let loadout = league_loadout_for_player(&league, &matrix_user_id);
    let progression = league_player_progression_json(&league, &player, &matrix_user_id);
    let guild = league
        .guild_memberships
        .get(&matrix_user_id)
        .and_then(|membership| league.guilds.get(&membership.guild_id))
        .cloned();
    (
        StatusCode::OK,
        Json(json!({
            "kind": "league_player_profile",
            "league": "trillionnium_league",
            "player": player,
            "guild": guild,
            "loadout": loadout,
            "progression": progression,
        })),
    )
        .into_response()
}

pub(super) async fn get_league_player_progression(
    Path(matrix_user_id): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }
    let mut league = state.inner.league_state.lock().await;
    let Some(matrix_user_id) = normalize_league_matrix_user(&matrix_user_id) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "matrix_user_id is required" })),
        )
            .into_response();
    };
    let player = ensure_league_player(&mut league, &matrix_user_id, None);
    let progression = league_player_progression_json(&league, &player, &matrix_user_id);
    (StatusCode::OK, Json(progression)).into_response()
}

pub(super) async fn get_league_player_loadout(
    Path(matrix_user_id): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }
    let mut league = state.inner.league_state.lock().await;
    let Some(matrix_user_id) = normalize_league_matrix_user(&matrix_user_id) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "matrix_user_id is required" })),
        )
            .into_response();
    };
    let player = ensure_league_player(&mut league, &matrix_user_id, None);
    let loadout = league_loadout_for_player(&league, &matrix_user_id);
    (
        StatusCode::OK,
        Json(json!({
            "kind": "league_loadout",
            "league": "trillionnium_league",
            "player": player,
            "loadout": loadout,
        })),
    )
        .into_response()
}

pub(super) async fn update_league_player_draft(
    Path(matrix_user_id): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<LeagueDraftRequest>,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }
    let Some(path_matrix_user_id) = normalize_league_matrix_user(&matrix_user_id) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "matrix_user_id is required" })),
        )
            .into_response();
    };
    let request_matrix_user_id = normalize_league_matrix_user(&payload.matrix_user_id)
        .unwrap_or_else(|| path_matrix_user_id.clone());
    if request_matrix_user_id != path_matrix_user_id {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "draft matrix_user_id does not match path" })),
        )
            .into_response();
    }
    let heroes = normalize_hero_draft(payload.heroes);
    if heroes.len() < 3 {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "draft requires at least 3 unique heroes" })),
        )
            .into_response();
    }
    let (player, loadout, snapshot) = {
        let mut league = state.inner.league_state.lock().await;
        let player = ensure_league_player(&mut league, &path_matrix_user_id, None);
        league
            .player_loadouts
            .insert(path_matrix_user_id.clone(), heroes.clone());
        let loadout = league_loadout_for_player(&league, &path_matrix_user_id);
        let snapshot = league.clone();
        (player, loadout, snapshot)
    };
    if let Err(response) = persist_league_state(&state, &snapshot).await {
        return response;
    }
    (
        StatusCode::OK,
        Json(json!({
            "kind": "league_draft",
            "league": "trillionnium_league",
            "player": player,
            "loadout": loadout,
        })),
    )
        .into_response()
}

pub(super) async fn get_league_player_rewards(
    Path(matrix_user_id): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }
    let Some(matrix_user_id) = normalize_league_matrix_user(&matrix_user_id) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "matrix_user_id is required" })),
        )
            .into_response();
    };
    let league = state.inner.league_state.lock().await;
    let rewards: Vec<LeagueReward> = league
        .rewards
        .iter()
        .filter(|reward| reward.matrix_user_id == matrix_user_id)
        .cloned()
        .collect();
    let total_earned: f64 = rewards
        .iter()
        .filter(|reward| {
            matches!(
                reward.ledger_status.as_deref(),
                Some("settled") | Some("duplicate")
            )
        })
        .map(|reward| reward.amount)
        .sum();
    (
        StatusCode::OK,
        Json(json!({
            "kind": "league_rewards",
            "league": "trillionnium_league",
            "matrix_user_id": matrix_user_id,
            "total_earned": total_earned,
            "currency_unit": "credit",
            "rewards": rewards,
        })),
    )
        .into_response()
}

pub(super) async fn get_league_player_inventory(
    Path(matrix_user_id): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }
    let Some(matrix_user_id) = normalize_league_matrix_user(&matrix_user_id) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "matrix_user_id is required" })),
        )
            .into_response();
    };
    let league = state.inner.league_state.lock().await;
    let mut items: Vec<LeagueInventoryItem> = league
        .inventory_items
        .iter()
        .filter(|item| item.matrix_user_id == matrix_user_id)
        .cloned()
        .collect();
    items.sort_by(|left, right| {
        right
            .power
            .cmp(&left.power)
            .then_with(|| right.created_at_epoch.cmp(&left.created_at_epoch))
    });
    let total_power: i64 = items.iter().map(|item| item.power).sum();
    (
        StatusCode::OK,
        Json(json!({
            "kind": "league_inventory",
            "league": "trillionnium_league",
            "matrix_user_id": matrix_user_id,
            "item_count": items.len(),
            "total_power": total_power,
            "items": items,
        })),
    )
        .into_response()
}

pub(super) async fn get_league_player_history(
    Path(matrix_user_id): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }
    let Some(matrix_user_id) = normalize_league_matrix_user(&matrix_user_id) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "matrix_user_id is required" })),
        )
            .into_response();
    };
    let league = state.inner.league_state.lock().await;
    let battles: Vec<LeagueBattle> = league
        .battles
        .values()
        .filter(|battle| battle.matrix_user_id == matrix_user_id)
        .cloned()
        .collect();
    let submissions: Vec<LeagueSubmission> = league
        .submissions
        .values()
        .filter(|submission| submission.matrix_user_id == matrix_user_id)
        .cloned()
        .collect();
    (
        StatusCode::OK,
        Json(json!({
            "kind": "league_history",
            "league": "trillionnium_league",
            "matrix_user_id": matrix_user_id,
            "battles": battles,
            "submissions": submissions,
        })),
    )
        .into_response()
}
