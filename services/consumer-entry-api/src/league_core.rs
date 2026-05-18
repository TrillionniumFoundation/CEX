use super::*;

pub(super) fn normalize_league_matrix_user(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

pub(super) fn escape_html_text(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

pub(super) fn contains_cjk_text(input: &str) -> bool {
    input.chars().any(|ch| {
        ('\u{3400}'..='\u{9fff}').contains(&ch) || ('\u{f900}'..='\u{faff}').contains(&ch)
    })
}

pub(super) fn contains_latin_text(input: &str) -> bool {
    input.chars().any(|ch| ch.is_ascii_alphabetic())
}

pub(super) fn i18n_span_from_bilingual_slash_copy(copy: &str) -> Option<String> {
    if !copy.contains(" / ") {
        return None;
    }
    fn normalize_language_piece(value: &str, keep_cjk: bool) -> String {
        fn cleanup(value: &str) -> String {
            value
                .trim_matches(|ch: char| {
                    ch.is_whitespace()
                        || matches!(
                            ch,
                            ':' | '：'
                                | ','
                                | '，'
                                | ';'
                                | '；'
                                | '。'
                                | '.'
                                | '!'
                                | '！'
                                | '?'
                                | '？'
                                | '、'
                                | '-'
                                | '·'
                        )
                })
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
        }
        fn strip_cjk(value: &str) -> String {
            value
                .chars()
                .filter(|ch| {
                    !(('\u{3400}'..='\u{9fff}').contains(ch)
                        || ('\u{f900}'..='\u{faff}').contains(ch)
                        || matches!(ch, '、' | '，' | '。' | '：' | '；' | '！' | '？'))
                })
                .collect::<String>()
        }
        if keep_cjk {
            let mut pieces = Vec::new();
            for raw_part in value.split('·') {
                let part = raw_part.trim();
                if !contains_cjk_text(part) {
                    continue;
                }
                if part.contains('：') || part.contains(':') {
                    let segments = part
                        .split(['：', ':'])
                        .map(str::trim)
                        .filter(|segment| contains_cjk_text(segment))
                        .collect::<Vec<_>>();
                    if !segments.is_empty() {
                        pieces.push(segments.join("："));
                        continue;
                    }
                }
                pieces.push(part.to_string());
            }
            return cleanup(&pieces.join(" · "));
        }
        cleanup(&strip_cjk(value))
    }

    let mut english_pieces = Vec::new();
    let mut chinese_pieces = Vec::new();
    for piece in copy.split(" / ") {
        let trimmed = piece.trim();
        if trimmed.is_empty() {
            continue;
        }
        let has_cjk = contains_cjk_text(trimmed);
        let has_latin = contains_latin_text(trimmed);
        if has_cjk && has_latin {
            let english = normalize_language_piece(trimmed, false);
            let chinese = normalize_language_piece(trimmed, true);
            if !english.is_empty() {
                english_pieces.push(english);
            }
            if !chinese.is_empty() {
                chinese_pieces.push(chinese);
            }
        } else if has_cjk {
            chinese_pieces.push(trimmed.to_string());
        } else if has_latin {
            english_pieces.push(trimmed.to_string());
        }
    }
    if english_pieces.is_empty() || chinese_pieces.is_empty() {
        return None;
    }
    let english = english_pieces.join(": ");
    let chinese = chinese_pieces.join("：");
    Some(format!(
        "<span data-i18n-en=\"{}\" data-i18n-zh=\"{}\">{}</span>",
        escape_html_text(&english),
        escape_html_text(&chinese),
        escape_html_text(&english)
    ))
}

pub(super) fn league_web_session_secret(config: &ConsumerEntryConfig) -> Option<&str> {
    config
        .league_web_session_secret
        .as_deref()
        .or(config.session_auth_secret.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

pub(super) fn league_web_csrf(
    secret: &str,
    matrix_user_id: &str,
    room_id: Option<&str>,
    issued_at: i64,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(secret.as_bytes());
    hasher.update(b":league-web-csrf:");
    hasher.update(matrix_user_id.as_bytes());
    hasher.update(b":");
    hasher.update(room_id.unwrap_or_default().as_bytes());
    hasher.update(b":");
    hasher.update(issued_at.to_string().as_bytes());
    URL_SAFE_NO_PAD.encode(hasher.finalize())
}

pub(super) fn encode_league_web_session(
    claims: &LeagueWebSessionClaims,
    secret: &str,
) -> Result<String, Response> {
    let assertion = URL_SAFE_NO_PAD.encode(serde_json::to_vec(claims).map_err(|err| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("failed to serialize web session: {err}") })),
        )
            .into_response()
    })?);
    let signature = sign_user_session_assertion(&assertion, secret)?;
    Ok(format!("{assertion}.{signature}"))
}

pub(super) fn cookie_value(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(header::COOKIE)
        .and_then(|value| value.to_str().ok())
        .and_then(|cookie_header| {
            cookie_header.split(';').find_map(|part| {
                let (key, value) = part.trim().split_once('=')?;
                (key == name).then(|| value.to_string())
            })
        })
}

pub(super) fn authorize_league_web_session(
    state: &AppState,
    headers: &HeaderMap,
    csrf: Option<&str>,
) -> Result<Option<LeagueWebSessionClaims>, Response> {
    authorize_league_web_session_inner(state, headers, csrf, false, true)
}

pub(super) fn authorize_league_web_session_readonly(
    state: &AppState,
    headers: &HeaderMap,
    allow_missing_cookie: bool,
) -> Result<Option<LeagueWebSessionClaims>, Response> {
    authorize_league_web_session_inner(state, headers, None, allow_missing_cookie, false)
}

fn authorize_league_web_session_inner(
    state: &AppState,
    headers: &HeaderMap,
    csrf: Option<&str>,
    allow_missing_cookie: bool,
    csrf_required_for_configured_session: bool,
) -> Result<Option<LeagueWebSessionClaims>, Response> {
    let Some(raw_cookie) = cookie_value(headers, &state.config().league_web_session_cookie_name)
    else {
        if state.config().league_web_session_required && !allow_missing_cookie {
            return Err((
                StatusCode::UNAUTHORIZED,
                Json(json!({
                    "error": "missing league web session cookie",
                    "session_endpoint": "/league/web/session",
                })),
            )
                .into_response());
        }
        return Ok(None);
    };
    let Some((assertion, signature)) = raw_cookie.split_once('.') else {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "invalid league web session cookie format" })),
        )
            .into_response());
    };
    let Some(secret) = league_web_session_secret(state.config()) else {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "league web session is required but no secret is configured" })),
        )
            .into_response());
    };
    let expected_signature = sign_user_session_assertion(assertion, secret)?;
    if expected_signature != signature {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "invalid league web session signature" })),
        )
            .into_response());
    }
    let bytes = URL_SAFE_NO_PAD.decode(assertion).map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "league web session assertion must be base64url JSON" })),
        )
            .into_response()
    })?;
    let claims = serde_json::from_slice::<LeagueWebSessionClaims>(&bytes).map_err(|err| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": format!("invalid league web session payload: {err}") })),
        )
            .into_response()
    })?;
    let now = Utc::now().timestamp();
    if claims.expires_at_epoch < now {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "league web session expired" })),
        )
            .into_response());
    }
    if let Some(provided_csrf) = csrf.map(str::trim).filter(|value| !value.is_empty()) {
        if provided_csrf != claims.csrf {
            return Err((
                StatusCode::FORBIDDEN,
                Json(json!({ "error": "league web csrf mismatch" })),
            )
                .into_response());
        }
    } else if state.config().league_web_session_required && csrf_required_for_configured_session {
        return Err((
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "league web csrf token is required" })),
        )
            .into_response());
    }
    if let Some(session_generation) = claims.game_account_session_generation {
        validate_game_account_session_generation(
            state,
            &claims.matrix_user_id,
            session_generation,
        )?;
    }
    Ok(Some(claims))
}

pub(super) fn league_hash_id(prefix: &str, value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    let encoded = URL_SAFE_NO_PAD.encode(hasher.finalize());
    format!("{prefix}-{}", &encoded[..16])
}

pub(super) fn league_entry_key(match_id: &str, matrix_user_id: &str) -> String {
    format!("{match_id}\u{1f}{matrix_user_id}")
}

pub(super) fn league_raid_roster_summary(league: &LeagueState, match_id: &str) -> Value {
    let mut slots: Vec<LeagueRaidRosterSlot> = league
        .raid_rosters
        .iter()
        .filter(|slot| slot.match_id == match_id && slot.status == "active")
        .cloned()
        .collect();
    slots.sort_by(|left, right| {
        left.role
            .cmp(&right.role)
            .then_with(|| left.matrix_user_id.cmp(&right.matrix_user_id))
    });
    let required_roles = ["scout", "builder", "auditor", "closer"];
    let filled_roles: Vec<String> = slots.iter().map(|slot| slot.role.clone()).collect();
    let missing_roles: Vec<&str> = required_roles
        .iter()
        .copied()
        .filter(|role| !filled_roles.iter().any(|filled| filled == role))
        .collect();
    json!({
        "match_id": match_id,
        "slots": slots,
        "slot_count": filled_roles.len(),
        "required_roles": required_roles,
        "missing_roles": missing_roles,
        "ready": missing_roles.is_empty(),
    })
}

pub(super) async fn record_league_raid_roster_slot(
    state: &AppState,
    match_id: &str,
    payload: LeagueRaidRosterRequest,
) -> Result<(LeagueState, LeagueRaidRosterSlot, Value), Response> {
    let matrix_user_id = match normalize_league_matrix_user(&payload.matrix_user_id) {
        Some(value) => value,
        None => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "matrix_user_id is required" })),
            )
                .into_response())
        }
    };
    let role = payload
        .role
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("scout")
        .to_ascii_lowercase()
        .replace('-', "_");
    let hero_id = payload
        .hero_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("oracle_scout")
        .to_ascii_lowercase()
        .replace('-', "_");
    let (snapshot, slot, roster) = {
        let mut league = state.inner.league_state.lock().await;
        let Some(raid_match) = league.matches.get(match_id).cloned() else {
            return Err((
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "league raid not found", "match_id": match_id })),
            )
                .into_response());
        };
        if raid_match.mode != "guild_raid" {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "match is not a guild raid", "match_id": match_id })),
            )
                .into_response());
        }
        let player = ensure_league_player(&mut league, &matrix_user_id, None);
        let guild_id = league
            .guild_memberships
            .get(&matrix_user_id)
            .map(|membership| membership.guild_id.clone());
        for existing in &mut league.raid_rosters {
            if existing.match_id == match_id && existing.matrix_user_id == matrix_user_id {
                existing.status = "replaced".to_string();
            }
        }
        let now = Utc::now().timestamp();
        let slot = LeagueRaidRosterSlot {
            slot_id: league_hash_id(
                "slot",
                &format!("{}:{}:{}:{}", match_id, matrix_user_id, role, now),
            ),
            match_id: match_id.to_string(),
            guild_id,
            player_id: player.player_id.clone(),
            matrix_user_id: matrix_user_id.clone(),
            room_id: payload.room_id.clone(),
            role,
            hero_id,
            status: "active".to_string(),
            joined_at_epoch: now,
        };
        league.raid_rosters.push(slot.clone());
        let roster = league_raid_roster_summary(&league, match_id);
        (league.clone(), slot, roster)
    };
    Ok((snapshot, slot, roster))
}

pub(super) fn league_raid_progress(league: &LeagueState, match_id: &str) -> Value {
    let contributions: Vec<&LeagueRaidContribution> = league
        .raid_contributions
        .iter()
        .filter(|contribution| contribution.match_id == match_id)
        .filter(|contribution| {
            contribution.payout_status.as_deref().unwrap_or("eligible") == "eligible"
        })
        .collect();
    let total_progress: f64 = contributions
        .iter()
        .map(|contribution| contribution.progress_delta)
        .sum::<f64>()
        .min(100.0);
    let average_score = if contributions.is_empty() {
        0.0
    } else {
        contributions
            .iter()
            .map(|contribution| contribution.contribution_score)
            .sum::<f64>()
            / contributions.len() as f64
    };
    let mut roles: Vec<String> = contributions
        .iter()
        .map(|contribution| contribution.role.clone())
        .collect();
    roles.sort();
    roles.dedup();
    json!({
        "match_id": match_id,
        "phase": if total_progress >= 100.0 { "cleared" } else if total_progress >= 60.0 { "boss" } else if total_progress >= 25.0 { "mid" } else { "opening" },
        "progress_percent": (total_progress * 10.0).round() / 10.0,
        "contribution_count": contributions.len(),
        "average_score": (average_score * 10.0).round() / 10.0,
        "roles": roles,
        "target_percent": 100.0,
    })
}

pub(super) async fn record_league_raid_contribution(
    state: &AppState,
    match_id: &str,
    payload: LeagueRaidContributionRequest,
) -> Result<(LeagueState, LeagueRaidContribution, Value), Response> {
    let matrix_user_id = match normalize_league_matrix_user(&payload.matrix_user_id) {
        Some(value) => value,
        None => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "matrix_user_id is required" })),
            )
                .into_response())
        }
    };
    let body = validate_text_payload(&payload.body, state.config().max_text_chars)?;
    let role = payload
        .role
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("raider")
        .to_string();
    let raid_mode = {
        let league = state.inner.league_state.lock().await;
        let Some(raid_match) = league.matches.get(match_id).cloned() else {
            return Err((
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "league raid not found", "match_id": match_id })),
            )
                .into_response());
        };
        if raid_match.mode != "guild_raid" {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "match is not a guild raid", "match_id": match_id })),
            )
                .into_response());
        }
        raid_match.mode
    };
    let judgement = judge_league_submission_with_pipeline(state, &body, &raid_mode).await;
    let (snapshot, contribution, progress) = {
        let mut league = state.inner.league_state.lock().await;
        if league
            .matches
            .get(match_id)
            .is_none_or(|league_match| league_match.mode != "guild_raid")
        {
            return Err((
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "league raid not found", "match_id": match_id })),
            )
                .into_response());
        }
        let mut player = ensure_league_player(&mut league, &matrix_user_id, None);
        let mut entry =
            ensure_league_entry(&mut league, match_id, &matrix_user_id, &player.player_id);
        let guild_id = league
            .guild_memberships
            .get(&matrix_user_id)
            .map(|membership| membership.guild_id.clone());
        let released = judgement.payout_status == "eligible";
        let progress_delta = if released {
            (judgement.score / 8.0).clamp(4.0, 14.0)
        } else {
            0.0
        };
        let now = Utc::now().timestamp();
        let contribution = LeagueRaidContribution {
            contribution_id: league_hash_id(
                "raid",
                &format!("{}:{}:{}:{}", match_id, matrix_user_id, now, body),
            ),
            match_id: match_id.to_string(),
            guild_id,
            player_id: player.player_id.clone(),
            matrix_user_id: matrix_user_id.clone(),
            room_id: payload.room_id.clone(),
            role,
            body: body.clone(),
            contribution_score: judgement.score,
            progress_delta,
            payout_status: Some(judgement.payout_status.clone()),
            anti_cheat_flags: judgement.anti_cheat_flags.clone(),
            score_events: judgement.score_events.clone(),
            created_at_epoch: now,
        };
        if released {
            player.xp += (judgement.score / 2.0).round() as i64;
            player.reputation += (judgement.score / 20.0).round() as i64;
            player.rating += ((judgement.score - 50.0) / 6.0).round() as i64;
            entry.battles_started += 1;
            league
                .players_by_matrix_user
                .insert(matrix_user_id.clone(), player);
            league
                .entries
                .insert(league_entry_key(match_id, &matrix_user_id), entry);
        }
        league.raid_contributions.push(contribution.clone());
        let progress = league_raid_progress(&league, match_id);
        (league.clone(), contribution, progress)
    };
    Ok((snapshot, contribution, progress))
}

pub(super) fn league_guild_standings(league: &LeagueState) -> Vec<Value> {
    let mut standings: Vec<Value> = league
        .guilds
        .values()
        .map(|guild| {
            let members: Vec<&LeagueGuildMembership> = league
                .guild_memberships
                .values()
                .filter(|membership| membership.guild_id == guild.guild_id)
                .collect();
            let member_count = members.len();
            let mut total_rating = guild.rating as f64;
            let mut total_earned = guild.treasury_credits;
            let mut submissions = 0_i64;
            for membership in members {
                if let Some(player) = league
                    .players_by_matrix_user
                    .get(&membership.matrix_user_id)
                {
                    total_rating += player.rating as f64;
                    total_earned += player.earned_credits;
                    submissions += player.submissions;
                }
            }
            let power_score = total_rating + total_earned * 10.0 + submissions as f64 * 25.0;
            json!({
                "guild_id": guild.guild_id,
                "name": guild.name,
                "member_count": member_count,
                "rating": guild.rating,
                "power_score": (power_score * 10.0).round() / 10.0,
                "earned_credits": (total_earned * 100.0).round() / 100.0,
                "submissions": submissions,
            })
        })
        .collect();
    standings.sort_by(|left, right| {
        let left_score = left
            .get("power_score")
            .and_then(Value::as_f64)
            .unwrap_or(0.0);
        let right_score = right
            .get("power_score")
            .and_then(Value::as_f64)
            .unwrap_or(0.0);
        right_score
            .partial_cmp(&left_score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                left.get("guild_id")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .cmp(
                        right
                            .get("guild_id")
                            .and_then(Value::as_str)
                            .unwrap_or_default(),
                    )
            })
    });
    standings
}

pub(super) fn ensure_league_player(
    league: &mut LeagueState,
    matrix_user_id: &str,
    display_name: Option<&str>,
) -> LeaguePlayer {
    if let Some(player) = league.players_by_matrix_user.get(matrix_user_id) {
        return player.clone();
    }
    let now = Utc::now().timestamp();
    let display_name = display_name
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(matrix_user_id)
        .to_string();
    let player = LeaguePlayer {
        player_id: league_hash_id("player", matrix_user_id),
        matrix_user_id: matrix_user_id.to_string(),
        display_name,
        class_tag: "summoner".to_string(),
        rank_tier: "Bronze I".to_string(),
        rating: 1000,
        xp: 0,
        reputation: 0,
        battles: 0,
        submissions: 0,
        wins: 0,
        earned_credits: 0.0,
        created_at_epoch: now,
    };
    league
        .players_by_matrix_user
        .insert(matrix_user_id.to_string(), player.clone());
    player
}

pub(super) fn ensure_league_entry(
    league: &mut LeagueState,
    match_id: &str,
    matrix_user_id: &str,
    player_id: &str,
) -> LeagueMatchEntry {
    let key = league_entry_key(match_id, matrix_user_id);
    if let Some(entry) = league.entries.get(&key) {
        return entry.clone();
    }
    let entry = LeagueMatchEntry {
        entry_id: league_hash_id("entry", &key),
        match_id: match_id.to_string(),
        player_id: player_id.to_string(),
        matrix_user_id: matrix_user_id.to_string(),
        status: "joined".to_string(),
        battles_started: 0,
        submissions: 0,
        best_score: 0.0,
        rewards_earned: 0.0,
        joined_at_epoch: Utc::now().timestamp(),
    };
    league.entries.insert(key, entry.clone());
    entry
}

pub(super) fn merge_league_battle_metadata(
    metadata: Option<Value>,
    match_id: &str,
    entry_id: &str,
) -> Value {
    let mut value = metadata.unwrap_or_else(|| json!({}));
    if !value.is_object() {
        value = json!({ "original_metadata": value });
    }
    if let Some(map) = value.as_object_mut() {
        map.insert(
            "trillionnium_league".to_string(),
            json!({
                "version": 1,
                "match_id": match_id,
                "entry_id": entry_id,
                "action": "battle",
            }),
        );
    }
    value
}

pub(super) fn league_item_for_submission(submission: &LeagueSubmission) -> LeagueInventoryItem {
    let (rarity, item_kind, name, power) = if submission.score >= 90.0 {
        ("mythic", "sigil", "Mythic Forge Sigil", 95)
    } else if submission.score >= 80.0 {
        ("epic", "badge", "Epic Quest Badge", 82)
    } else if submission.score >= 70.0 {
        ("rare", "charm", "Rare Evidence Charm", 68)
    } else {
        ("common", "token", "League Practice Token", 45)
    };
    LeagueInventoryItem {
        item_id: league_hash_id("item", &submission.submission_id),
        player_id: submission.player_id.clone(),
        matrix_user_id: submission.matrix_user_id.clone(),
        source_submission_id: submission.submission_id.clone(),
        item_kind: item_kind.to_string(),
        name: name.to_string(),
        rarity: rarity.to_string(),
        power,
        cosmetic: true,
        created_at_epoch: submission.created_at_epoch,
    }
}

fn league_encounter_state_event(body: &str, mode: &str) -> LeagueScoreEvent {
    let lower = body.to_ascii_lowercase();
    let required_roles: Vec<&str> = match mode {
        "guild_raid" => vec!["scout", "builder", "auditor", "closer"],
        "bounty_arena" => vec!["scout", "auditor", "closer"],
        "world_work_delivery" => vec!["builder", "auditor", "closer"],
        _ => vec!["scout", "builder", "closer"],
    };
    let covered_roles = required_roles
        .iter()
        .filter(|role| lower.contains(**role) || body.contains("分工") || body.contains("团队"))
        .count();
    let opponent_pressure = match mode {
        "bounty_arena" => 86,
        "guild_raid" => 78,
        "world_work_delivery" => 72,
        _ => 64,
    };
    let phase = if lower.contains("risk") || body.contains("风险") {
        "counterplay_locked"
    } else if lower.contains("evidence") || body.contains("证据") {
        "proof_window"
    } else {
        "opening_read"
    };
    let encounter_score = (((covered_roles as f64 / required_roles.len() as f64) * 50.0)
        + if phase == "counterplay_locked" {
            50.0
        } else {
            30.0
        })
    .min(100.0);
    LeagueScoreEvent {
        dimension: "encounter_state".to_string(),
        score: (encounter_score * 10.0).round() / 10.0,
        weight: 0.0,
        judge_kind: "encounter_state_v1".to_string(),
        evidence: json!({
            "contract_version": "trillionnium_league_encounter_state_v1",
            "mode": mode,
            "phase": phase,
            "opponent_pressure": opponent_pressure,
            "required_roles": required_roles,
            "covered_role_count": covered_roles,
            "player_counterplay": ["deliverable", "evidence", "risk_control", "next_action", "team_role"],
            "next_turn_hint": "Pick a role, answer the pressure point, submit evidence, then lock reward or recovery.",
        }),
    }
}

pub(super) fn judge_league_submission(body: &str, mode: &str) -> LeagueJudgement {
    let chars = body.chars().count() as f64;
    let lower = body.to_ascii_lowercase();
    let has_deliver = lower.contains("deliver")
        || lower.contains("customer")
        || body.contains("客户")
        || body.contains("交付")
        || body.contains("方案");
    let has_evidence = lower.contains("evidence")
        || lower.contains("source")
        || lower.contains("data")
        || body.contains("证据")
        || body.contains("依据");
    let has_risk = lower.contains("risk") || body.contains("风险");
    let has_review = lower.contains("review") || body.contains("自评") || body.contains("复盘");
    let has_next = lower.contains("next") || body.contains("下一步") || body.contains("计划");

    let delivery_score = if has_deliver { 86.0 } else { 54.0 };
    let evidence_score = if has_evidence { 82.0 } else { 48.0 };
    let risk_score = if has_risk { 80.0 } else { 46.0 };
    let action_score = if has_next { 84.0 } else { 52.0 };
    let polish_score =
        ((chars / 1.4).clamp(40.0, 88.0) + if has_review { 8.0 } else { 0.0 }).min(96.0);

    let mut events = vec![
        LeagueScoreEvent {
            dimension: "delivery_fit".to_string(),
            score: delivery_score,
            weight: 0.30,
            judge_kind: "rubric_v1".to_string(),
            evidence: json!({"has_deliverable": has_deliver}),
        },
        LeagueScoreEvent {
            dimension: "evidence_grounding".to_string(),
            score: evidence_score,
            weight: 0.24,
            judge_kind: "rubric_v1".to_string(),
            evidence: json!({"has_evidence": has_evidence}),
        },
        LeagueScoreEvent {
            dimension: "risk_control".to_string(),
            score: risk_score,
            weight: 0.18,
            judge_kind: "rubric_v1".to_string(),
            evidence: json!({"has_risk": has_risk}),
        },
        LeagueScoreEvent {
            dimension: "actionability".to_string(),
            score: action_score,
            weight: 0.16,
            judge_kind: "rubric_v1".to_string(),
            evidence: json!({"has_next_step": has_next}),
        },
        LeagueScoreEvent {
            dimension: "craft_polish".to_string(),
            score: polish_score,
            weight: 0.12,
            judge_kind: "rubric_v1".to_string(),
            evidence: json!({"chars": chars, "has_self_review": has_review}),
        },
    ];
    events.push(league_encounter_state_event(body, mode));
    let score = events
        .iter()
        .map(|event| event.score * event.weight)
        .sum::<f64>()
        .clamp(0.0, 100.0);
    let score = (score * 10.0).round() / 10.0;
    let grade = league_judgement_grade(score);
    let mut anti_cheat_flags = Vec::new();
    if chars < 24.0 {
        anti_cheat_flags.push("too_short".to_string());
    }
    if lower.matches("copy").count() >= 3 || body.matches('复').count() >= 12 {
        anti_cheat_flags.push("repetition_suspected".to_string());
    }
    let mode_multiplier = match mode {
        "bounty_arena" => 1.5,
        "guild_raid" => 1.25,
        _ => 1.0,
    };
    let reward_amount = ((score / 20.0) * mode_multiplier * 100.0).round() / 100.0;
    LeagueJudgement {
        score,
        grade,
        reward_amount,
        judge_status: "rubric_scored".to_string(),
        payout_status: if anti_cheat_flags.is_empty() {
            "eligible".to_string()
        } else {
            "review_hold".to_string()
        },
        anti_cheat_flags,
        score_events: events,
    }
}

pub(super) fn league_judgement_grade(score: f64) -> String {
    if score >= 90.0 {
        "S"
    } else if score >= 80.0 {
        "A"
    } else if score >= 70.0 {
        "B"
    } else if score >= 60.0 {
        "C"
    } else {
        "D"
    }
    .to_string()
}

pub(super) fn league_hidden_test_event(body: &str, mode: &str) -> (LeagueScoreEvent, Vec<String>) {
    let lower = body.to_ascii_lowercase();
    let mut tests = vec![
        json!({"id": "deliverable_anchor", "passed": lower.contains("deliver") || lower.contains("customer") || body.contains("客户") || body.contains("交付") || body.contains("方案")}),
        json!({"id": "evidence_anchor", "passed": lower.contains("evidence") || lower.contains("source") || lower.contains("data") || body.contains("证据") || body.contains("依据")}),
        json!({"id": "risk_gate", "passed": lower.contains("risk") || body.contains("风险")}),
        json!({"id": "next_action", "passed": lower.contains("next") || body.contains("下一步") || body.contains("计划")}),
        json!({"id": "substance_length", "passed": body.chars().count() >= 40}),
        json!({"id": "not_repetitive", "passed": lower.matches("copy").count() < 3 && body.matches('复').count() < 12}),
    ];
    if mode == "guild_raid" {
        tests.push(json!({"id": "raid_coordination", "passed": lower.contains("scout") || lower.contains("builder") || lower.contains("team") || lower.contains("gate") || body.contains("团队") || body.contains("分工")}));
    }
    let passed = tests
        .iter()
        .filter(|test| test.get("passed").and_then(Value::as_bool).unwrap_or(false))
        .count();
    let score = ((passed as f64 / tests.len() as f64) * 1000.0).round() / 10.0;
    let mut flags = Vec::new();
    if score < 70.0 {
        flags.push("hidden_tests_failed".to_string());
    }
    if tests.iter().any(|test| {
        test.get("id").and_then(Value::as_str) == Some("evidence_anchor")
            && !test.get("passed").and_then(Value::as_bool).unwrap_or(false)
    }) {
        flags.push("hidden_missing_evidence".to_string());
    }
    (
        LeagueScoreEvent {
            dimension: "hidden_tests".to_string(),
            score,
            weight: 0.0,
            judge_kind: "hidden_tests_v1".to_string(),
            evidence: json!({
                "mode": mode,
                "passed": passed,
                "total": tests.len(),
                "tests": tests,
            }),
        },
        flags,
    )
}

pub(super) async fn call_league_llm_judge_adapter(
    state: &AppState,
    body: &str,
    mode: &str,
    judgement: &LeagueJudgement,
) -> LeagueExternalJudgeOutcome {
    let Some(url) = state.config().league_llm_judge_url.clone() else {
        return LeagueExternalJudgeOutcome {
            event: LeagueScoreEvent {
                dimension: "llm_judge_adapter".to_string(),
                score: judgement.score,
                weight: 0.0,
                judge_kind: "llm_adapter_v1".to_string(),
                evidence: json!({"status": "not_configured"}),
            },
            flags: Vec::new(),
            grade_override: None,
        };
    };
    let mut request = state
        .inner
        .http
        .post(url.trim())
        .timeout(Duration::from_millis(
            state.config().league_llm_judge_timeout_ms,
        ))
        .json(&json!({
            "league": "trillionnium_league",
            "mode": mode,
            "submission": {"body": body},
            "rubric": {
                "score": judgement.score,
                "grade": &judgement.grade,
                "events": &judgement.score_events,
            }
        }));
    if let Some(token) = state.config().league_llm_judge_token.as_deref() {
        request = request.bearer_auth(token);
    }
    let required = state.config().league_llm_judge_required;
    let response = match request.send().await {
        Ok(response) => response,
        Err(err) => {
            return LeagueExternalJudgeOutcome {
                event: LeagueScoreEvent {
                    dimension: "llm_judge_adapter".to_string(),
                    score: judgement.score,
                    weight: 0.0,
                    judge_kind: "llm_adapter_v1".to_string(),
                    evidence: json!({"status": "network_error", "error": err.to_string()}),
                },
                flags: if required {
                    vec!["judge_adapter_unavailable".to_string()]
                } else {
                    Vec::new()
                },
                grade_override: None,
            }
        }
    };
    let status = response.status();
    if !status.is_success() {
        return LeagueExternalJudgeOutcome {
            event: LeagueScoreEvent {
                dimension: "llm_judge_adapter".to_string(),
                score: judgement.score,
                weight: 0.0,
                judge_kind: "llm_adapter_v1".to_string(),
                evidence: json!({"status": "http_error", "http_status": status.as_u16()}),
            },
            flags: if required {
                vec!["judge_adapter_unavailable".to_string()]
            } else {
                Vec::new()
            },
            grade_override: None,
        };
    }
    let parsed = match response.json::<LeagueExternalJudgeResponse>().await {
        Ok(value) => value,
        Err(err) => {
            return LeagueExternalJudgeOutcome {
                event: LeagueScoreEvent {
                    dimension: "llm_judge_adapter".to_string(),
                    score: judgement.score,
                    weight: 0.0,
                    judge_kind: "llm_adapter_v1".to_string(),
                    evidence: json!({"status": "bad_json", "error": err.to_string()}),
                },
                flags: if required {
                    vec!["judge_adapter_unavailable".to_string()]
                } else {
                    Vec::new()
                },
                grade_override: None,
            }
        }
    };
    let adapter_score = parsed.score.unwrap_or(judgement.score).clamp(0.0, 100.0);
    let delta = ((adapter_score - judgement.score).abs() * 10.0).round() / 10.0;
    let mut flags = parsed.flags.unwrap_or_default();
    if delta >= 18.0 {
        flags.push("judge_disagreement".to_string());
    }
    LeagueExternalJudgeOutcome {
        event: LeagueScoreEvent {
            dimension: "llm_judge_adapter".to_string(),
            score: (adapter_score * 10.0).round() / 10.0,
            weight: 0.0,
            judge_kind: "llm_adapter_v1".to_string(),
            evidence: json!({
                "status": "scored",
                "delta_from_rubric": delta,
                "verdict": parsed.verdict,
                "explanation": parsed.explanation,
                "evidence": parsed.evidence,
            }),
        },
        flags,
        grade_override: parsed.grade,
    }
}

pub(super) async fn judge_league_submission_with_pipeline(
    state: &AppState,
    body: &str,
    mode: &str,
) -> LeagueJudgement {
    let mut judgement = judge_league_submission(body, mode);
    if state.config().league_hidden_tests_enabled {
        let (event, mut flags) = league_hidden_test_event(body, mode);
        judgement.score_events.push(event);
        judgement.anti_cheat_flags.append(&mut flags);
    }
    let external = call_league_llm_judge_adapter(state, body, mode, &judgement).await;
    judgement.score_events.push(external.event);
    judgement.anti_cheat_flags.extend(external.flags);
    if let Some(grade) = external
        .grade_override
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    {
        judgement.grade = grade;
    }
    judgement.anti_cheat_flags.sort();
    judgement.anti_cheat_flags.dedup();
    judgement.payout_status = if judgement.anti_cheat_flags.is_empty() {
        "eligible".to_string()
    } else {
        "review_hold".to_string()
    };
    judgement.judge_status = if state.config().league_llm_judge_url.is_some() {
        "rubric_hidden_llm_pipeline_v2".to_string()
    } else {
        "rubric_hidden_pipeline_v2".to_string()
    };
    judgement
}

pub(super) fn default_league_loadout() -> Value {
    loadout_json_from_heroes(&[
        "oracle_scout".to_string(),
        "forge_builder".to_string(),
        "mirror_auditor".to_string(),
        "courier_closer".to_string(),
    ])
}

pub(super) fn league_loadout_for_player(league: &LeagueState, matrix_user_id: &str) -> Value {
    league
        .player_loadouts
        .get(matrix_user_id)
        .map(|heroes| loadout_json_from_heroes(heroes))
        .unwrap_or_else(default_league_loadout)
}

pub(super) fn league_reward_receipt_progression_allows(
    league: &LeagueState,
    reward: &LeagueReward,
) -> Option<bool> {
    let intent_id = format!("league_reward:{}", reward.reward_id);
    league
        .term_exchange_receipts
        .values()
        .filter(|receipt| receipt.intent_id == intent_id)
        .max_by(|left, right| {
            left.finalized_at_epoch
                .cmp(&right.finalized_at_epoch)
                .then_with(|| left.receipt_id.cmp(&right.receipt_id))
        })
        .map(|receipt| {
            receipt.progression_class
                == term_exchange_protocol::ReceiptProgressionClass::ProgressionAllowed
        })
}

pub(super) fn league_reward_ledger_released_from_state(
    league: &LeagueState,
    reward: &LeagueReward,
) -> bool {
    if let Some(typed_released) = league_reward_receipt_progression_allows(league, reward) {
        return typed_released;
    }
    matches!(
        reward.ledger_status.as_deref(),
        Some("settled") | Some("duplicate")
    )
}

pub(super) fn league_successful_task_count(league: &LeagueState, matrix_user_id: &str) -> i64 {
    let league_submissions = league
        .submissions
        .values()
        .filter(|submission| {
            let payout_status = submission.payout_status.as_deref().unwrap_or("eligible");
            let released = payout_status == "eligible" || payout_status == "approved_release";
            let ledger_released = league.rewards.iter().any(|reward| {
                reward.reward_id == league_hash_id("reward", &submission.submission_id)
                    && league_reward_ledger_released_from_state(league, reward)
            });
            submission.matrix_user_id == matrix_user_id
                && submission.score >= 60.0
                && released
                && ledger_released
        })
        .count() as i64;
    let contract_completions = league
        .world
        .world_contract_completions
        .iter()
        .filter(|completion| {
            completion.matrix_user_id == matrix_user_id
                && completion.score >= 60.0
                && completion.payout_status == "eligible"
                && world_commerce_routes::world_contract_completion_released(
                    &league.world,
                    completion,
                )
        })
        .count() as i64;
    let work_deliveries = league
        .world
        .world_work_deliveries
        .iter()
        .filter(|delivery| {
            delivery.matrix_user_id == matrix_user_id
                && delivery.score >= 60.0
                && delivery.status == "delivered"
        })
        .count() as i64;
    let accepted_work = league
        .world
        .world_work_acceptances
        .iter()
        .filter(|acceptance| {
            acceptance.matrix_user_id == matrix_user_id && acceptance.status == "accepted"
        })
        .count() as i64;
    let asset_upgrades = league
        .world
        .world_asset_upgrades
        .iter()
        .filter(|upgrade| {
            upgrade.matrix_user_id == matrix_user_id
                && upgrade.score >= 60.0
                && upgrade.status == "upgraded"
        })
        .count() as i64;
    league_submissions + contract_completions + work_deliveries + accepted_work + asset_upgrades
}

pub(super) fn league_experience_data_points(league: &LeagueState, matrix_user_id: &str) -> i64 {
    let battles = league
        .battles
        .values()
        .filter(|battle| battle.matrix_user_id == matrix_user_id)
        .count() as i64;
    let submissions = league
        .submissions
        .values()
        .filter(|submission| submission.matrix_user_id == matrix_user_id)
        .count() as i64;
    let world_events = league
        .world
        .world_events
        .iter()
        .filter(|event| event.actor_matrix_user_id == matrix_user_id)
        .count() as i64;
    let assets = league
        .world
        .world_assets
        .iter()
        .filter(|asset| asset.owner_matrix_user_id == matrix_user_id)
        .count() as i64;
    let companies = league
        .world
        .world_companies
        .iter()
        .filter(|company| company.owner_matrix_user_id == matrix_user_id)
        .count() as i64;
    let listings = league
        .world
        .world_listings
        .iter()
        .filter(|listing| listing.owner_matrix_user_id == matrix_user_id)
        .count() as i64;
    let purchases = league
        .world
        .world_purchases
        .iter()
        .filter(|purchase| {
            purchase.buyer_matrix_user_id == matrix_user_id
                || purchase.seller_matrix_user_id == matrix_user_id
        })
        .count() as i64;
    battles + submissions + world_events + assets + companies + listings + purchases
}

pub(super) fn league_level_from_successful_tasks(successful_task_count: i64) -> i64 {
    (successful_task_count / 3 + 1).clamp(1, 100)
}

pub(super) fn league_rank_for_level(level: i64) -> &'static str {
    match level {
        1..=2 => "Apprentice",
        3..=5 => "Adept",
        6..=9 => "Expert",
        10..=14 => "Master",
        15..=24 => "Grandmaster",
        _ => "Mythic Founder",
    }
}

pub(super) fn league_player_progression_json(
    league: &LeagueState,
    player: &LeaguePlayer,
    matrix_user_id: &str,
) -> Value {
    let successful_task_count = league_successful_task_count(league, matrix_user_id);
    let experience_data_points = league_experience_data_points(league, matrix_user_id);
    let world_indexes = build_world_indexes(&league.world);
    let level = league_level_from_successful_tasks(successful_task_count);
    let next_level_successes = level * 3;
    let successes_to_next_level = (next_level_successes - successful_task_count).max(0);
    let loadout = league_loadout_for_player(league, matrix_user_id);
    let loadout_agent_count = loadout
        .get("heroes")
        .and_then(Value::as_array)
        .map(|heroes| heroes.len() as i64)
        .unwrap_or(0);
    let membership = league.guild_memberships.get(matrix_user_id);
    let current_school = membership
        .and_then(|membership| league.guilds.get(&membership.guild_id))
        .map(|guild| {
            json!({
                "school_id": guild.guild_id,
                "name": guild.name,
                "school_kind": "guild_school",
                "role": membership.map(|value| value.role.clone()).unwrap_or_else(|| "member".to_string()),
                "rank": league_rank_for_level(level),
            })
        })
        .unwrap_or_else(|| {
            json!({
                "school_id": "faction-city-clerks",
                "name": "City Clerks",
                "school_kind": "starter_faction",
                "role": "wanderer",
                "rank": league_rank_for_level(level),
            })
        });

    let mut schools: Vec<Value> = league
        .guilds
        .values()
        .map(|guild| {
            json!({
                "school_id": guild.guild_id,
                "name": guild.name,
                "school_kind": "guild_school",
                "status": guild.status,
                "reputation": guild.reputation,
                "joined": membership.map(|value| value.guild_id.as_str()) == Some(guild.guild_id.as_str()),
            })
        })
        .collect();
    schools.extend(
        world_indexes
            .sorted_faction_ids
            .iter()
            .filter_map(|faction_id| league.world.world_factions.get(faction_id))
            .map(|faction| {
                json!({
                    "school_id": faction.faction_id,
                    "name": faction.name,
                    "school_kind": faction.faction_kind,
                    "status": faction.status,
                    "reputation": faction.reputation_score,
                    "joined": false,
                })
            }),
    );
    schools.sort_by(|left, right| {
        left.get("school_id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .cmp(
                right
                    .get("school_id")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
            )
    });

    let mut skill_catalog: Vec<LeagueSkill> = league.league_skills.values().cloned().collect();
    skill_catalog.sort_by(|left, right| {
        left.unlock_level
            .cmp(&right.unlock_level)
            .then_with(|| left.skill_id.cmp(&right.skill_id))
    });
    let skills: Vec<Value> = skill_catalog
        .into_iter()
        .map(|skill| {
            let unlocked = level >= skill.unlock_level;
            let rank = if unlocked {
                (1 + (level - skill.unlock_level) / 2).min(skill.max_rank)
            } else {
                0
            };
            json!({
                "skill_id": skill.skill_id,
                "name": skill.name,
                "skill_kind": skill.skill_kind,
                "school_id": skill.school_id,
                "description": skill.description,
                "unlock_level": skill.unlock_level,
                "max_rank": skill.max_rank,
                "rank": rank,
                "unlocked": unlocked,
            })
        })
        .collect();

    let mut tool_catalog: Vec<LeagueTool> = league.league_tools.values().cloned().collect();
    tool_catalog.sort_by(|left, right| {
        left.unlock_level
            .cmp(&right.unlock_level)
            .then_with(|| left.tool_id.cmp(&right.tool_id))
    });
    let earned_items: Vec<&LeagueInventoryItem> = league
        .inventory_items
        .iter()
        .filter(|item| item.matrix_user_id == matrix_user_id)
        .collect();
    let earned_power: i64 = earned_items.iter().map(|item| item.power).sum();
    let tools: Vec<Value> = tool_catalog
        .into_iter()
        .map(|tool| {
            let unlocked = level >= tool.unlock_level;
            json!({
                "tool_id": tool.tool_id,
                "name": tool.name,
                "tool_kind": tool.tool_kind,
                "slot": tool.slot,
                "description": tool.description,
                "unlock_level": tool.unlock_level,
                "power_bonus": tool.power_bonus,
                "unlocked": unlocked,
            })
        })
        .collect();

    let mut skin_catalog: Vec<LeagueSkin> = league.league_skins.values().cloned().collect();
    skin_catalog.sort_by(|left, right| {
        left.unlock_level
            .cmp(&right.unlock_level)
            .then_with(|| left.skin_id.cmp(&right.skin_id))
    });
    let skins: Vec<Value> = skin_catalog
        .into_iter()
        .map(|skin| {
            let unlocked =
                level >= skin.unlock_level && loadout_agent_count >= skin.agent_count.min(5);
            json!({
                "skin_id": skin.skin_id,
                "name": skin.name,
                "skin_kind": skin.skin_kind,
                "description": skin.description,
                "unlock_level": skin.unlock_level,
                "agent_count": skin.agent_count,
                "multi_agent_capability": skin.multi_agent_capability,
                "unlocked": unlocked,
            })
        })
        .collect();

    let unlocked_skill_count = skills
        .iter()
        .filter(|skill| {
            skill
                .get("unlocked")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        })
        .count();
    let unlocked_tool_count = tools
        .iter()
        .filter(|tool| {
            tool.get("unlocked")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        })
        .count();
    let unlocked_skin_count = skins
        .iter()
        .filter(|skin| {
            skin.get("unlocked")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        })
        .count();

    json!({
        "kind": "league_progression",
        "league": "trillionnium_league",
        "player": player,
        "current_school": current_school,
        "schools": schools,
        "successful_task_count": successful_task_count,
        "successes_to_next_level": successes_to_next_level,
        "experience_data_points": experience_data_points,
        "level": level,
        "level_basis": "successful_task_count",
        "rank_title": league_rank_for_level(level),
        "xp": player.xp,
        "skills": skills,
        "skill_count": skills.len(),
        "unlocked_skill_count": unlocked_skill_count,
        "tools": tools,
        "tool_count": tools.len(),
        "unlocked_tool_count": unlocked_tool_count,
        "earned_item_count": earned_items.len(),
        "earned_item_power": earned_power,
        "skins": skins,
        "skin_count": skins.len(),
        "unlocked_skin_count": unlocked_skin_count,
        "loadout_agent_count": loadout_agent_count,
    })
}

pub(super) fn normalize_hero_draft(values: Vec<String>) -> Vec<String> {
    let mut heroes = Vec::new();
    for value in values {
        let hero = value.trim().to_ascii_lowercase().replace('-', "_");
        if hero.is_empty() || heroes.contains(&hero) {
            continue;
        }
        heroes.push(hero);
        if heroes.len() >= 5 {
            break;
        }
    }
    heroes
}

pub(super) fn loadout_json_from_heroes(heroes: &[String]) -> Value {
    let hero_values: Vec<Value> = heroes
        .iter()
        .map(|hero| {
            let (name, role) = match hero.as_str() {
                "oracle_scout" => ("Oracle Scout", "侦察/调研"),
                "forge_builder" => ("Forge Builder", "生成/构建"),
                "mirror_auditor" => ("Mirror Auditor", "审核/测试"),
                "courier_closer" => ("Courier Closer", "交付/包装"),
                "ledger_warden" => ("Ledger Warden", "成本/风控"),
                "muse_designer" => ("Muse Designer", "视觉/设计"),
                _ => (hero.as_str(), "自定义英雄"),
            };
            json!({"hero_id": hero, "name": name, "role": role})
        })
        .collect();
    json!({
        "heroes": hero_values,
        "draft_unlocked": true,
        "max_slots": 5,
    })
}
