use super::*;

const GAME_ACCOUNT_CLIENT_CONTRACT: &str = "trillionnium_game_account_client_v1";

pub(super) async fn get_game_account_client_shell_response(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    let web_session = authorize_league_web_session_readonly(&state, &headers, true)
        .ok()
        .flatten();
    let html = game_account_client_shell_html(&state, web_session.as_ref());
    html_resource_response(html, GAME_ACCOUNT_CLIENT_CONTRACT)
}

fn game_account_client_shell_html(
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
    } else if matches!(state.config().runtime_profile, RuntimeProfile::LocalDev) {
        "local_dev_session_can_be_minted_from_player_id"
    } else {
        "signed_upstream_user_session_required"
    };
    let production_requires_signed_upstream =
        !matches!(state.config().runtime_profile, RuntimeProfile::LocalDev);
    let readiness = json!({
        "contract_version": GAME_ACCOUNT_CLIENT_CONTRACT,
        "status": "game_account_client_shell_ready",
        "client_surface": "/account",
        "alternate_surface": "/game/account",
        "session_endpoint": "/league/web/session",
        "session_cookie": {
            "name": state.config().league_web_session_cookie_name,
            "http_only": true,
            "same_site": "Lax",
            "csrf_required_for_mutations": true
        },
        "flows": {
            "register": {
                "form_id": "account-register-form",
                "client_storage": "localStorage:trillionnium.account.profile.v1",
                "server_bridge": "/league/web/session",
                "password_auth_implemented": false,
                "credential_storage_in_browser": false
            },
            "login": {
                "form_id": "account-login-form",
                "server_bridge": "/league/web/session",
                "password_auth_implemented": false,
                "credential_storage_in_browser": false
            },
            "logout": {
                "button_id": "account-logout-button",
                "client_clears_local_profile": true,
                "server_cookie_expiry_endpoint": "not_yet_implemented"
            }
        },
        "session_state": {
            "active": session_active,
            "mode": session_mode,
            "matrix_user_id": web_session.map(|session| session.matrix_user_id.as_str()),
            "room_id": web_session.and_then(|session| session.room_id.as_deref()),
            "session_id": web_session.and_then(|session| session.session_id.as_deref())
        },
        "production_boundary": {
            "requires_signed_upstream_user_session": production_requires_signed_upstream,
            "self_serve_password_registration_backend": false,
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
    html.push_str("<main id=\"game-account-client\" data-public-launch-credit=\"false\" data-client-submits-intent-only=\"true\">\n");
    html.push_str("  <header>\n    <div>\n");
    html.push_str("      <p class=\"muted\">Trillionnium World</p>\n");
    html.push_str("      <h1>Player Account</h1>\n");
    html.push_str("    </div>\n");
    html.push_str("    <a class=\"link-button\" href=\"/app\">Open Game</a>\n");
    html.push_str("  </header>\n");
    html.push_str("  <section class=\"grid\">\n");
    html.push_str("    <div class=\"panel\">\n");
    html.push_str("      <h2>Register or Sign In</h2>\n");
    html.push_str("      <p class=\"muted\">This client creates a signed game web session through <code>/league/web/session</code>. In production, that bridge requires an upstream signed user session; the browser never stores passwords or ingress tokens.</p>\n");
    html.push_str("      <div class=\"forms\">\n");
    html.push_str("        <form id=\"account-register-form\" data-account-flow=\"register\" data-session-endpoint=\"/league/web/session\">\n");
    html.push_str("          <h3>Create player profile</h3>\n");
    html.push_str("          <label>Player ID <input name=\"matrix_user_id\" autocomplete=\"username\" placeholder=\"@player:trillionnium.local\" required /></label>\n");
    html.push_str("          <label>Display name <input name=\"display_name\" autocomplete=\"nickname\" placeholder=\"Player name\" /></label>\n");
    html.push_str("          <label>Room ID <input name=\"room_id\" placeholder=\"!lobby:trillionnium.local\" /></label>\n");
    html.push_str("          <label>Session ID <input name=\"session_id\" placeholder=\"first-device\" /></label>\n");
    html.push_str(
        "          <div class=\"actions\"><button type=\"submit\">Create session</button></div>\n",
    );
    html.push_str("        </form>\n");
    html.push_str("        <form id=\"account-login-form\" data-account-flow=\"login\" data-session-endpoint=\"/league/web/session\">\n");
    html.push_str("          <h3>Sign in</h3>\n");
    html.push_str("          <label>Player ID <input name=\"matrix_user_id\" autocomplete=\"username\" placeholder=\"@player:trillionnium.local\" required /></label>\n");
    html.push_str("          <label>Room ID <input name=\"room_id\" placeholder=\"!lobby:trillionnium.local\" /></label>\n");
    html.push_str("          <label>Session ID <input name=\"session_id\" placeholder=\"returning-device\" /></label>\n");
    html.push_str("          <div class=\"actions\"><button type=\"submit\">Sign in</button><button id=\"account-logout-button\" class=\"secondary\" type=\"button\">Forget local profile</button></div>\n");
    html.push_str("        </form>\n");
    html.push_str("      </div>\n");
    html.push_str("      <p class=\"muted\">Password registration is intentionally not implemented in this shell; production login is delegated to the signed upstream user-session issuer before a game session is minted.</p>\n");
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
    html.push_str("    <p>This is the game account client shell and session bridge. It does not claim self-serve password auth, public-launch readiness, or authority over gameplay state.</p>\n");
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
        for (const key of ["matrix_user_id", "room_id", "session_id"]) {
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
      matrix_user_id: String(data.matrix_user_id || "").trim(),
      room_id: String(data.room_id || "").trim() || null,
      session_id: String(data.session_id || "").trim() || form.dataset.accountFlow
    };
    if (!payload.matrix_user_id) {
      updateStatus("Player ID is required.", "warn");
      return;
    }
    localStorage.setItem(profileKey, JSON.stringify({ ...payload, display_name: data.display_name || "" }));
    try {
      const response = await fetch("/league/web/session", {
        method: "POST",
        credentials: "same-origin",
        headers: { "content-type": "application/json" },
        body: JSON.stringify(payload)
      });
      if (!response.ok) {
        const body = await response.text();
        updateStatus("Session bridge rejected the request (" + response.status + "). " + body, "warn");
        return;
      }
      updateStatus("Signed game session created. Open the game, world, or league surface.", "status");
    } catch (error) {
      updateStatus("Session request failed: " + error, "warn");
    }
  };
  for (const form of document.querySelectorAll("form[data-account-flow]")) {
    form.addEventListener("submit", submit);
  }
  document.getElementById("account-logout-button")?.addEventListener("click", () => {
    localStorage.removeItem(profileKey);
    updateStatus("Local profile cleared. Server HttpOnly session cookies expire by TTL.", "status");
  });
  restoreProfile();
})();
"#);
    html.push_str("  </script>\n");
    html.push_str("</main>\n</body>\n</html>\n");
    html
}
