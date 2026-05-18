use super::*;
use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};

pub(super) const GAME_ACCOUNT_CLIENT_CONTRACT: &str = "trillionnium_game_account_client_v1";
const GAME_ACCOUNT_PASSWORD_AUTH_CONTRACT: &str = "trillionnium_game_account_password_auth_v1";
const GAME_ACCOUNT_SESSION_STATUS_CONTRACT: &str = "trillionnium_game_account_session_status_v1";
const GAME_ACCOUNT_LOCAL_PROFILE_KEY: &str = "trillionnium.account.profile.v1";
const GAME_ACCOUNT_DEFAULT_DOMAIN: &str = "trillionnium.local";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct GameAccountRegistry {
    version: u64,
    accounts: HashMap<String, GameAccountRecord>,
}

impl Default for GameAccountRegistry {
    fn default() -> Self {
        Self {
            version: 1,
            accounts: HashMap::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct GameAccountRecord {
    matrix_user_id: String,
    display_name: Option<String>,
    room_id: Option<String>,
    password_hash: String,
    created_at_epoch: i64,
    updated_at_epoch: i64,
    last_login_epoch: Option<i64>,
    disabled: bool,
}

#[derive(Debug, Deserialize)]
pub(super) struct GameAccountPasswordRequest {
    matrix_user_id: Option<String>,
    handle: Option<String>,
    display_name: Option<String>,
    password: String,
    room_id: Option<String>,
    session_id: Option<String>,
}

pub(super) fn load_game_account_registry(config: &ConsumerEntryConfig) -> GameAccountRegistry {
    let Some(path) = config.game_account_registry_path.as_deref() else {
        return GameAccountRegistry::default();
    };
    let Ok(bytes) = fs::read(path) else {
        return GameAccountRegistry::default();
    };
    serde_json::from_slice::<GameAccountRegistry>(&bytes).unwrap_or_default()
}

pub(super) async fn get_game_account_client_shell_response(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    let web_session = authorize_league_web_session_readonly(&state, &headers, true)
        .ok()
        .flatten();
    let html = game_account_client_shell_html(&state, web_session.as_ref()).await;
    html_resource_response(html, GAME_ACCOUNT_CLIENT_CONTRACT)
}

pub(super) async fn get_game_account_session_status(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    match authorize_league_web_session_readonly(&state, &headers, true) {
        Ok(session) => {
            Json(game_account_session_status_json(&state, session.as_ref()).await).into_response()
        }
        Err(response) => response,
    }
}

pub(super) async fn post_game_account_logout(State(state): State<AppState>) -> Response {
    state.inner.metrics.inc_game_account_logout_successes();
    (
        StatusCode::OK,
        [(
            header::SET_COOKIE,
            game_account_session_clear_cookie(state.config()),
        )],
        Json(json!({
            "kind": "game_account_logout",
            "contract_version": GAME_ACCOUNT_SESSION_STATUS_CONTRACT,
            "logged_out": true,
            "session_cookie_cleared": true,
        })),
    )
        .into_response()
}

pub(super) async fn post_game_account_register(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<GameAccountPasswordRequest>,
) -> Response {
    if let Err(response) = ensure_game_account_password_auth_enabled(&state) {
        return response;
    }
    let matrix_user_id =
        match normalize_game_account_matrix_user(&payload, state.config()) {
            Some(value) => value,
            None => return (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "error": "valid handle or matrix_user_id is required",
                    "expected": "handle with 3-40 letters/digits/._- or explicit @user:domain id",
                })),
            )
                .into_response(),
        };
    if let Err(response) =
        enforce_game_account_auth_rate_limits(&state, &headers, "register", &matrix_user_id).await
    {
        return response;
    }
    if let Err(response) = validate_game_account_password(&payload.password, state.config()) {
        return response;
    }

    let now = Utc::now().timestamp();
    let password_hash = match hash_game_account_password(&payload.password) {
        Ok(value) => value,
        Err(response) => return response,
    };
    let display_name = normalize_optional_account_text(payload.display_name.as_deref());
    let room_id = normalize_optional_account_text(payload.room_id.as_deref());
    let session_id = normalize_optional_account_text(payload.session_id.as_deref())
        .or_else(|| Some("registered-browser".to_string()));
    let record = GameAccountRecord {
        matrix_user_id: matrix_user_id.clone(),
        display_name: display_name.clone(),
        room_id: room_id.clone(),
        password_hash,
        created_at_epoch: now,
        updated_at_epoch: now,
        last_login_epoch: Some(now),
        disabled: false,
    };

    {
        let mut registry = state.inner.game_account_registry.lock().await;
        if registry.accounts.contains_key(&matrix_user_id) {
            return (
                StatusCode::CONFLICT,
                Json(json!({ "error": "game account already exists" })),
            )
                .into_response();
        }
        registry.accounts.insert(matrix_user_id.clone(), record);
        if let Err(err) = persist_game_account_registry(state.config(), &registry) {
            registry.accounts.remove(&matrix_user_id);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("failed to persist game account registry: {err}") })),
            )
                .into_response();
        }
    }

    state.inner.metrics.inc_game_account_register_successes();
    issue_game_account_session_response(
        &state,
        matrix_user_id,
        display_name,
        room_id,
        session_id,
        "registered",
    )
}

pub(super) async fn post_game_account_login(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<GameAccountPasswordRequest>,
) -> Response {
    if let Err(response) = ensure_game_account_password_auth_enabled(&state) {
        return response;
    }
    let matrix_user_id = match normalize_game_account_matrix_user(&payload, state.config()) {
        Some(value) => value,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "valid handle or matrix_user_id is required" })),
            )
                .into_response()
        }
    };
    if let Err(response) =
        enforce_game_account_auth_rate_limits(&state, &headers, "login", &matrix_user_id).await
    {
        return response;
    }
    let record = {
        let registry = state.inner.game_account_registry.lock().await;
        registry.accounts.get(&matrix_user_id).cloned()
    };
    let Some(mut record) = record.filter(|record| !record.disabled) else {
        state.inner.metrics.inc_game_account_login_failures();
        return invalid_game_account_credentials_response();
    };
    if !verify_game_account_password(&payload.password, &record.password_hash) {
        state.inner.metrics.inc_game_account_login_failures();
        return invalid_game_account_credentials_response();
    }

    let now = Utc::now().timestamp();
    record.last_login_epoch = Some(now);
    record.updated_at_epoch = now;
    let room_id =
        normalize_optional_account_text(payload.room_id.as_deref()).or(record.room_id.clone());
    let session_id = normalize_optional_account_text(payload.session_id.as_deref())
        .or_else(|| Some("returning-browser".to_string()));
    {
        let mut registry = state.inner.game_account_registry.lock().await;
        registry
            .accounts
            .insert(matrix_user_id.clone(), record.clone());
        if let Err(err) = persist_game_account_registry(state.config(), &registry) {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("failed to persist game account login: {err}") })),
            )
                .into_response();
        }
    }

    state.inner.metrics.inc_game_account_login_successes();
    issue_game_account_session_response(
        &state,
        matrix_user_id,
        record.display_name,
        room_id,
        session_id,
        "logged_in",
    )
}

async fn game_account_client_shell_html(
    state: &AppState,
    web_session: Option<&LeagueWebSessionClaims>,
) -> String {
    let session_active = web_session.is_some();
    let session_player = web_session
        .map(|session| session.matrix_user_id.as_str())
        .unwrap_or("not signed in");
    let session_room = web_session
        .and_then(|session| session.room_id.as_deref())
        .unwrap_or("none");
    let session_id = web_session
        .and_then(|session| session.session_id.as_deref())
        .unwrap_or("none");
    let session_mode = if session_active {
        "signed_game_session_active"
    } else if state.config().game_account_password_auth_enabled {
        "self_serve_game_account_password_auth_available"
    } else if matches!(state.config().runtime_profile, RuntimeProfile::LocalDev) {
        "local_dev_session_can_be_minted_from_player_id"
    } else {
        "signed_upstream_user_session_required"
    };
    let production_requires_signed_upstream =
        !matches!(state.config().runtime_profile, RuntimeProfile::LocalDev)
            && !state.config().game_account_password_auth_enabled;
    let account_count = {
        let registry = state.inner.game_account_registry.lock().await;
        registry.accounts.len()
    };
    let password_auth_enabled = state.config().game_account_password_auth_enabled;
    let registry_persistence = state
        .config()
        .game_account_registry_path
        .as_deref()
        .unwrap_or("memory_only");
    let readiness = json!({
        "contract_version": GAME_ACCOUNT_CLIENT_CONTRACT,
        "status": "game_account_client_shell_ready",
        "client_surface": "/account",
        "alternate_surface": "/game/account",
        "session_endpoint": "/league/web/session",
        "session_status_endpoint": "/account/session",
        "register_endpoint": "/account/register",
        "login_endpoint": "/account/login",
        "logout_endpoint": "/account/logout",
        "password_auth_contract_version": GAME_ACCOUNT_PASSWORD_AUTH_CONTRACT,
        "session_status_contract_version": GAME_ACCOUNT_SESSION_STATUS_CONTRACT,
        "session_cookie": {
            "name": state.config().league_web_session_cookie_name,
            "http_only": true,
            "same_site": "Lax",
            "csrf_required_for_gameplay_mutations": true
        },
        "flows": {
            "register": {
                "form_id": "account-register-form",
                "endpoint": "/account/register",
                "client_storage": format!("localStorage:{GAME_ACCOUNT_LOCAL_PROFILE_KEY}"),
                "password_auth_implemented": true,
                "password_auth_enabled": password_auth_enabled,
                "password_hash": "argon2id",
                "credential_storage_in_browser": false
            },
            "login": {
                "form_id": "account-login-form",
                "endpoint": "/account/login",
                "password_auth_implemented": true,
                "password_auth_enabled": password_auth_enabled,
                "password_hash": "argon2id",
                "credential_storage_in_browser": false
            },
            "logout": {
                "button_id": "account-logout-button",
                "endpoint": "/account/logout",
                "client_clears_local_profile": true,
                "server_cookie_expiry_endpoint": "/account/logout"
            }
        },
        "session_state": {
            "active": session_active,
            "mode": session_mode,
            "matrix_user_id": web_session.map(|session| session.matrix_user_id.as_str()),
            "room_id": web_session.and_then(|session| session.room_id.as_deref()),
            "session_id": web_session.and_then(|session| session.session_id.as_deref())
        },
        "password_auth": {
            "enabled": password_auth_enabled,
            "registry_persistence": registry_persistence,
            "registered_account_count": account_count,
            "minimum_password_chars": state.config().game_account_password_min_chars,
            "auth_rate_limit_max_requests": state.config().game_account_auth_rate_limit_max_requests,
            "auth_rate_limit_window_secs": state.config().rate_limit_window_secs,
            "default_local_domain": state.config().game_account_local_domain
        },
        "observability": {
            "contract_version": "trillionnium_game_account_auth_observability_v1",
            "metrics_endpoint": "/metrics",
            "health_metrics_path": "/health.metrics.game_account_auth",
            "event_scope": "aggregate_no_password_no_token",
            "passwords_tokens_or_cookie_values_logged": false
        },
        "production_boundary": {
            "requires_signed_upstream_user_session_when_password_auth_disabled": production_requires_signed_upstream,
            "self_serve_password_registration_backend": password_auth_enabled,
            "public_launch_credit": false,
            "client_submits_intent_only": true
        },
        "next_routes": ["/app", "/world", "/league"]
    });
    let readiness_json = serde_json::to_string_pretty(&readiness)
        .unwrap_or_else(|_| "{}".to_string())
        .replace('<', "\\u003c")
        .replace('>', "\\u003e")
        .replace('&', "\\u0026");
    let active_attr = if session_active { "true" } else { "false" };
    let production_attr = if production_requires_signed_upstream {
        "true"
    } else {
        "false"
    };
    let password_auth_attr = if password_auth_enabled {
        "true"
    } else {
        "false"
    };
    let disabled_attr = if password_auth_enabled {
        ""
    } else {
        " disabled"
    };

    let mut html = String::new();
    html.push_str("<!doctype html>\n<html lang=\"en\">\n<head>\n");
    html.push_str("  <meta charset=\"utf-8\" />\n");
    html.push_str("  <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\" />\n");
    html.push_str("  <title>Trillionnium Account</title>\n");
    html.push_str("  <style>\n");
    html.push_str("    :root { color-scheme: light; font-family: Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, \"Segoe UI\", sans-serif; background: #f6f7f9; color: #18212f; }\n");
    html.push_str("    body { margin: 0; min-height: 100vh; background: #f6f7f9; }\n");
    html.push_str("    main { width: min(1080px, calc(100vw - 32px)); margin: 0 auto; padding: 32px 0 48px; }\n");
    html.push_str("    header { display: flex; align-items: flex-end; justify-content: space-between; gap: 16px; margin-bottom: 20px; }\n");
    html.push_str("    h1 { margin: 0; font-size: clamp(1.7rem, 2.5vw, 2.35rem); line-height: 1.05; letter-spacing: 0; }\n");
    html.push_str("    p { line-height: 1.55; }\n");
    html.push_str("    .grid { display: grid; grid-template-columns: minmax(0, 1fr) minmax(280px, 360px); gap: 16px; align-items: start; }\n");
    html.push_str("    .panel { background: #fff; border: 1px solid #dce2ea; border-radius: 8px; padding: 18px; box-shadow: 0 1px 2px rgba(15, 23, 42, .04); }\n");
    html.push_str("    .forms { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 14px; }\n");
    html.push_str("    label { display: grid; gap: 6px; margin-top: 10px; font-size: .88rem; font-weight: 650; color: #334155; }\n");
    html.push_str("    input { width: 100%; box-sizing: border-box; border: 1px solid #cbd5e1; border-radius: 6px; padding: 10px 11px; font: inherit; }\n");
    html.push_str("    button, .link-button { min-height: 40px; border: 0; border-radius: 6px; padding: 10px 13px; background: #172033; color: #fff; font-weight: 750; cursor: pointer; text-decoration: none; display: inline-flex; align-items: center; justify-content: center; }\n");
    html.push_str("    button.secondary { background: #e8edf4; color: #172033; }\n");
    html.push_str(
        "    button:disabled { background: #cbd5e1; color: #64748b; cursor: not-allowed; }\n",
    );
    html.push_str(
        "    .actions { display: flex; flex-wrap: wrap; gap: 10px; margin-top: 14px; }\n",
    );
    html.push_str("    .muted { color: #64748b; font-size: .92rem; }\n");
    html.push_str("    .status { border-left: 4px solid #2563eb; background: #eff6ff; }\n");
    html.push_str("    .warn { border-left: 4px solid #d97706; background: #fffbeb; }\n");
    html.push_str("    code { background: #eef2f7; border-radius: 4px; padding: 2px 5px; }\n");
    html.push_str("    ul { padding-left: 18px; }\n");
    html.push_str("    @media (max-width: 820px) { header, .grid, .forms { display: block; } .panel { margin-top: 14px; } }\n");
    html.push_str("  </style>\n</head>\n");
    html.push_str("<body data-contract=\"trillionnium_game_account_client_v1\">\n");
    html.push_str("<main id=\"game-account-client\" data-public-launch-credit=\"false\" data-client-submits-intent-only=\"true\" data-password-auth-implemented=\"true\" data-password-auth-enabled=\"");
    html.push_str(password_auth_attr);
    html.push_str("\">\n");
    html.push_str("  <header>\n    <div>\n");
    html.push_str("      <p class=\"muted\">Trillionnium World</p>\n");
    html.push_str("      <h1>Player Account</h1>\n");
    html.push_str("    </div>\n");
    html.push_str("    <a class=\"link-button\" href=\"/app\">Open Game</a>\n");
    html.push_str("  </header>\n");
    html.push_str("  <section class=\"grid\">\n");
    html.push_str("    <div class=\"panel\">\n");
    html.push_str("      <h2>Register or Sign In</h2>\n");
    html.push_str("      <p class=\"muted\">This client creates a signed game web session. Self-serve password auth uses Argon2id when enabled; otherwise production delegates session minting to the signed upstream user-session issuer.</p>\n");
    html.push_str("      <div class=\"forms\">\n");
    html.push_str("        <form id=\"account-register-form\" data-account-flow=\"register\" data-account-endpoint=\"/account/register\" data-session-endpoint=\"/league/web/session\">\n");
    html.push_str("          <h3>Create player profile</h3>\n");
    html.push_str("          <label>Handle <input name=\"handle\" autocomplete=\"username\" placeholder=\"playername\" required /></label>\n");
    html.push_str("          <label>Display name <input name=\"display_name\" autocomplete=\"nickname\" placeholder=\"Player name\" /></label>\n");
    html.push_str("          <label>Password <input name=\"password\" type=\"password\" autocomplete=\"new-password\" minlength=\"");
    html.push_str(&state.config().game_account_password_min_chars.to_string());
    html.push_str("\" required /></label>\n");
    html.push_str("          <label>Room ID <input name=\"room_id\" placeholder=\"!lobby:trillionnium.local\" /></label>\n");
    html.push_str("          <label>Session ID <input name=\"session_id\" placeholder=\"first-device\" /></label>\n");
    html.push_str("          <div class=\"actions\"><button type=\"submit\"");
    html.push_str(disabled_attr);
    html.push_str(">Create account</button></div>\n");
    html.push_str("        </form>\n");
    html.push_str("        <form id=\"account-login-form\" data-account-flow=\"login\" data-account-endpoint=\"/account/login\" data-session-endpoint=\"/league/web/session\">\n");
    html.push_str("          <h3>Sign in</h3>\n");
    html.push_str("          <label>Handle or Player ID <input name=\"handle\" autocomplete=\"username\" placeholder=\"playername or @player:domain\" required /></label>\n");
    html.push_str("          <label>Password <input name=\"password\" type=\"password\" autocomplete=\"current-password\" required /></label>\n");
    html.push_str("          <label>Room ID <input name=\"room_id\" placeholder=\"!lobby:trillionnium.local\" /></label>\n");
    html.push_str("          <label>Session ID <input name=\"session_id\" placeholder=\"returning-device\" /></label>\n");
    html.push_str("          <div class=\"actions\"><button type=\"submit\"");
    html.push_str(disabled_attr);
    html.push_str(">Sign in</button><button id=\"account-logout-button\" class=\"secondary\" type=\"button\">Log out</button></div>\n");
    html.push_str("        </form>\n");
    html.push_str("      </div>\n");
    html.push_str("      <p class=\"muted\">The browser never stores passwords or game-session tokens. The server session cookie is HttpOnly, SameSite=Lax, and used with CSRF for gameplay mutations.</p>\n");
    html.push_str("    </div>\n");
    html.push_str("    <aside class=\"panel status\" id=\"account-session-status\" aria-live=\"polite\" data-session-active=\"");
    html.push_str(active_attr);
    html.push_str("\" data-production-requires-signed-upstream=\"");
    html.push_str(production_attr);
    html.push_str("\">\n");
    html.push_str("      <h2>Session State</h2>\n");
    html.push_str("      <ul>\n");
    html.push_str("        <li>mode: <code>");
    html.push_str(&escape_html_text(session_mode));
    html.push_str("</code></li>\n");
    html.push_str("        <li>player: <code>");
    html.push_str(&escape_html_text(session_player));
    html.push_str("</code></li>\n");
    html.push_str("        <li>room: <code>");
    html.push_str(&escape_html_text(session_room));
    html.push_str("</code></li>\n");
    html.push_str("        <li>session: <code>");
    html.push_str(&escape_html_text(session_id));
    html.push_str("</code></li>\n");
    html.push_str("      </ul>\n");
    html.push_str("      <div class=\"actions\"><a class=\"link-button\" href=\"/world\">World</a><a class=\"link-button\" href=\"/league\">League</a></div>\n");
    html.push_str("    </aside>\n");
    html.push_str("  </section>\n");
    html.push_str("  <section class=\"panel warn\" id=\"account-production-boundary\">\n");
    html.push_str("    <h2>Boundary</h2>\n");
    html.push_str("    <p>This is the game account client and signed session bridge. It does not claim public-launch readiness or authority over gameplay state.</p>\n");
    html.push_str("  </section>\n");
    html.push_str("  <script id=\"account-client-readiness\" type=\"application/json\">");
    html.push_str(&readiness_json);
    html.push_str("</script>\n");
    html.push_str("  <script>\n");
    html.push_str(r#"
(() => {
  const profileKey = "trillionnium.account.profile.v1";
  const status = document.getElementById("account-session-status");
  const updateStatus = (message, tone = "status") => {
    status.classList.remove("status", "warn");
    status.classList.add(tone);
    status.setAttribute("data-last-client-message", message);
    const note = document.createElement("p");
    note.className = "muted";
    note.textContent = message;
    status.appendChild(note);
  };
  const restoreProfile = () => {
    try {
      const profile = JSON.parse(localStorage.getItem(profileKey) || "{}");
      for (const form of document.querySelectorAll("form[data-account-flow]")) {
        for (const key of ["handle", "matrix_user_id", "room_id", "session_id"]) {
          if (profile[key] && form.elements[key]) form.elements[key].value = profile[key];
        }
      }
    } catch (_) {}
  };
  const submit = async (event) => {
    event.preventDefault();
    const form = event.currentTarget;
    const data = Object.fromEntries(new FormData(form).entries());
    const payload = {
      handle: String(data.handle || "").trim() || null,
      matrix_user_id: String(data.matrix_user_id || "").trim() || null,
      display_name: String(data.display_name || "").trim() || null,
      password: String(data.password || ""),
      room_id: String(data.room_id || "").trim() || null,
      session_id: String(data.session_id || "").trim() || form.dataset.accountFlow
    };
    if (!payload.handle && !payload.matrix_user_id) {
      updateStatus("Handle or Player ID is required.", "warn");
      return;
    }
    try {
      const response = await fetch(form.dataset.accountEndpoint, {
        method: "POST",
        credentials: "same-origin",
        headers: { "content-type": "application/json" },
        body: JSON.stringify(payload)
      });
      const body = await response.json().catch(async () => ({ error: await response.text() }));
      if (!response.ok) {
        updateStatus("Account request rejected (" + response.status + "). " + (body.error || "unknown error"), "warn");
        return;
      }
      localStorage.setItem(profileKey, JSON.stringify({
        handle: payload.handle,
        matrix_user_id: body.matrix_user_id || payload.matrix_user_id,
        display_name: body.display_name || payload.display_name || "",
        room_id: body.room_id || payload.room_id || "",
        session_id: body.session_id || payload.session_id || ""
      }));
      status.setAttribute("data-session-active", "true");
      updateStatus("Signed game session ready. Open the game, world, or league surface.", "status");
    } catch (error) {
      updateStatus("Account request failed: " + error, "warn");
    }
  };
  for (const form of document.querySelectorAll("form[data-account-flow]")) {
    form.addEventListener("submit", submit);
  }
  document.getElementById("account-logout-button")?.addEventListener("click", async () => {
    localStorage.removeItem(profileKey);
    try {
      await fetch("/account/logout", { method: "POST", credentials: "same-origin" });
    } catch (_) {}
    status.setAttribute("data-session-active", "false");
    updateStatus("Signed game session cleared.", "status");
  });
  restoreProfile();
})();
"#);
    html.push_str("  </script>\n");
    html.push_str("</main>\n</body>\n</html>\n");
    html
}

async fn game_account_session_status_json(
    state: &AppState,
    web_session: Option<&LeagueWebSessionClaims>,
) -> Value {
    let profile = if let Some(session) = web_session {
        let registry = state.inner.game_account_registry.lock().await;
        registry
            .accounts
            .get(&session.matrix_user_id)
            .map(|record| {
                json!({
                    "matrix_user_id": record.matrix_user_id,
                    "display_name": record.display_name,
                    "room_id": record.room_id,
                    "created_at_epoch": record.created_at_epoch,
                    "last_login_epoch": record.last_login_epoch,
                    "disabled": record.disabled,
                })
            })
    } else {
        None
    };
    json!({
        "kind": "game_account_session_status",
        "contract_version": GAME_ACCOUNT_SESSION_STATUS_CONTRACT,
        "active": web_session.is_some(),
        "session": web_session.map(|session| json!({
            "matrix_user_id": session.matrix_user_id,
            "room_id": session.room_id,
            "session_id": session.session_id,
            "expires_at_epoch": session.expires_at_epoch,
            "csrf": session.csrf,
        })),
        "profile": profile,
        "password_auth": {
            "implemented": true,
            "enabled": state.config().game_account_password_auth_enabled,
            "register_endpoint": "/account/register",
            "login_endpoint": "/account/login",
            "logout_endpoint": "/account/logout",
        },
        "public_launch_credit": false,
    })
}

fn ensure_game_account_password_auth_enabled(state: &AppState) -> Result<(), Response> {
    if !state.config().game_account_password_auth_enabled {
        return Err((
            StatusCode::FORBIDDEN,
            Json(json!({
                "error": "game account password auth is disabled",
                "enable_with": "CONSUMER_ENTRY_GAME_ACCOUNT_PASSWORD_AUTH_ENABLED=true",
                "session_endpoint": "/league/web/session",
            })),
        )
            .into_response());
    }
    if league_web_session_secret(state.config()).is_none() {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "league web session secret is required for account login" })),
        )
            .into_response());
    }
    Ok(())
}

async fn enforce_game_account_auth_rate_limits(
    state: &AppState,
    headers: &HeaderMap,
    action: &str,
    matrix_user_id: &str,
) -> Result<(), Response> {
    let max_requests = state.config().game_account_auth_rate_limit_max_requests;
    let account_key = format!("game-account-auth:{action}:account:{matrix_user_id}");
    enforce_rate_limit(
        state,
        account_key,
        max_requests,
        "consumer_entry_game_account_auth_rate_limited",
        RateLimitBucketKind::User,
    )
    .await
    .map_err(|response| {
        state.inner.metrics.inc_game_account_auth_rate_limited();
        response
    })?;

    let source_key = format!(
        "game-account-auth:{action}:source:{}",
        game_account_auth_source_key(headers)
    );
    enforce_rate_limit(
        state,
        source_key,
        max_requests,
        "consumer_entry_game_account_source_rate_limited",
        RateLimitBucketKind::SourceScope,
    )
    .await
    .map_err(|response| {
        state.inner.metrics.inc_game_account_auth_rate_limited();
        response
    })
}

fn game_account_auth_source_key(headers: &HeaderMap) -> String {
    headers
        .get("x-forwarded-for")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(',').next())
        .or_else(|| {
            headers
                .get("x-real-ip")
                .and_then(|value| value.to_str().ok())
        })
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("direct")
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '.' | ':' | '-' | '_') {
                ch
            } else {
                '_'
            }
        })
        .take(96)
        .collect()
}

fn normalize_game_account_matrix_user(
    payload: &GameAccountPasswordRequest,
    config: &ConsumerEntryConfig,
) -> Option<String> {
    let raw = payload
        .matrix_user_id
        .as_deref()
        .or(payload.handle.as_deref())?
        .trim();
    if raw.starts_with('@') {
        return normalize_league_matrix_user(raw);
    }
    let handle = raw.trim_start_matches('@').to_ascii_lowercase();
    if !(3..=40).contains(&handle.len()) {
        return None;
    }
    if !handle
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.'))
    {
        return None;
    }
    let domain = config
        .game_account_local_domain
        .trim()
        .trim_start_matches(':')
        .trim_start_matches('@')
        .trim();
    let domain = if domain.is_empty() {
        GAME_ACCOUNT_DEFAULT_DOMAIN
    } else {
        domain
    };
    Some(format!("@{}:{}", handle, domain))
}

fn normalize_optional_account_text(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.chars().take(160).collect::<String>())
}

fn validate_game_account_password(
    password: &str,
    config: &ConsumerEntryConfig,
) -> Result<(), Response> {
    if password.chars().count() < config.game_account_password_min_chars {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": "password is too short",
                "minimum_chars": config.game_account_password_min_chars,
            })),
        )
            .into_response());
    }
    Ok(())
}

fn hash_game_account_password(password: &str) -> Result<String, Response> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|err| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("failed to hash account password: {err}") })),
            )
                .into_response()
        })
}

fn verify_game_account_password(password: &str, password_hash: &str) -> bool {
    let Ok(parsed_hash) = PasswordHash::new(password_hash) else {
        return false;
    };
    Argon2::default()
        .verify_password(password.as_bytes(), &parsed_hash)
        .is_ok()
}

fn invalid_game_account_credentials_response() -> Response {
    (
        StatusCode::UNAUTHORIZED,
        Json(json!({ "error": "invalid game account credentials" })),
    )
        .into_response()
}

fn persist_game_account_registry(
    config: &ConsumerEntryConfig,
    registry: &GameAccountRegistry,
) -> Result<(), String> {
    let Some(path) = config.game_account_registry_path.as_deref() else {
        return Ok(());
    };
    if let Some(parent) = StdPath::new(path).parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    let bytes = serde_json::to_vec_pretty(registry).map_err(|err| err.to_string())?;
    fs::write(path, bytes).map_err(|err| err.to_string())
}

fn issue_game_account_session_response(
    state: &AppState,
    matrix_user_id: String,
    display_name: Option<String>,
    room_id: Option<String>,
    session_id: Option<String>,
    status: &'static str,
) -> Response {
    let Some(secret) = league_web_session_secret(state.config()) else {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "league web session secret is not configured" })),
        )
            .into_response();
    };
    let now = Utc::now().timestamp();
    let csrf = league_web_csrf(secret, &matrix_user_id, room_id.as_deref(), now);
    let claims = LeagueWebSessionClaims {
        version: 1,
        matrix_user_id: matrix_user_id.clone(),
        room_id: room_id.clone(),
        session_id: session_id.clone(),
        csrf,
        issued_at_epoch: now,
        expires_at_epoch: now + state.config().league_web_session_ttl_secs as i64,
    };
    let token = match encode_league_web_session(&claims, secret) {
        Ok(value) => value,
        Err(response) => return response,
    };
    (
        StatusCode::OK,
        [(
            header::SET_COOKIE,
            game_account_session_cookie(state.config(), &token),
        )],
        Json(json!({
            "kind": "game_account_session",
            "contract_version": GAME_ACCOUNT_PASSWORD_AUTH_CONTRACT,
            "status": status,
            "matrix_user_id": matrix_user_id,
            "display_name": display_name,
            "room_id": room_id,
            "session_id": session_id,
            "csrf": claims.csrf,
            "expires_at_epoch": claims.expires_at_epoch,
            "session_status_endpoint": "/account/session",
            "logout_endpoint": "/account/logout",
            "public_launch_credit": false,
        })),
    )
        .into_response()
}

fn game_account_session_cookie(config: &ConsumerEntryConfig, token: &str) -> String {
    let secure = if matches!(config.runtime_profile, RuntimeProfile::LocalDev) {
        ""
    } else {
        "; Secure"
    };
    format!(
        "{}={}; Path=/; HttpOnly; SameSite=Lax; Max-Age={}{}",
        config.league_web_session_cookie_name, token, config.league_web_session_ttl_secs, secure,
    )
}

fn game_account_session_clear_cookie(config: &ConsumerEntryConfig) -> String {
    let secure = if matches!(config.runtime_profile, RuntimeProfile::LocalDev) {
        ""
    } else {
        "; Secure"
    };
    format!(
        "{}=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0{}",
        config.league_web_session_cookie_name, secure,
    )
}
