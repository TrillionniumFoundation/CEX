//! Immutable sync-stream description; credential configuration is not read.
//! This describes configuration, not a proof of membership/history coverage.
use reqwest::Url;
use serde_json::{json, Value};
use std::env::VarError;

pub(super) fn configured_filter(value: Result<String, VarError>) -> Result<Option<String>, &'static str> {
    match value {
        Ok(value) if !value.trim().is_empty() && value.len() <= 4096 => Ok(Some(value)),
        Err(VarError::NotPresent) => Ok(None),
        _ => Err("matrix_stream_filter_invalid"),
    }
}

pub(super) fn describe(
    homeserver: &str,
    bot_user_id: &str,
    filter: Option<&str>,
) -> Result<Value, &'static str> {
    if homeserver.len() > 2048 {
        return Err("matrix_stream_endpoint_invalid");
    }
    let url = endpoint(homeserver)?;
    let valid_user = bot_user_id.strip_prefix('@').and_then(|id| id.split_once(':'))
        .is_some_and(|(local, server)| !local.is_empty() && !server.is_empty());
    if !valid_user
        || bot_user_id.len() > 512
        || bot_user_id.chars().any(|c| c.is_control() || c.is_whitespace())
    {
        return Err("matrix_stream_user_invalid");
    }
    let filter = match filter {
        None => json!({"kind": "none"}),
        Some(raw) => {
            if raw.is_empty() || raw.len() > 4096 {
                return Err("matrix_stream_filter_invalid");
            }
            let trimmed = raw.trim();
            if trimmed.starts_with('{') {
                let value: Value = serde_json::from_str(trimmed)
                    .map_err(|_| "matrix_stream_filter_invalid")?;
                if !value.is_object() {
                    return Err("matrix_stream_filter_invalid");
                }
                // Keep the exact filter bytes: parser differences and duplicate JSON keys
                // must not be normalized into an apparently unchanged server filter.
                json!({"kind": "inline", "value": raw})
            } else {
                // Opaque server-side filter IDs are not interpreted as integers.
                if raw != trimmed || raw.len() > 1024
                    || raw.chars().any(|c| c.is_control() || c.is_whitespace())
                    || (raw.starts_with('[') || raw.starts_with('"'))
                {
                    return Err("matrix_stream_filter_invalid");
                }
                json!({"kind": "id", "value": raw})
            }
        }
    };
    Ok(json!({
        "schema": "cex.matrix.stream-scope.v1",
        "homeserver": url.as_str().trim_end_matches('/'),
        "bot_user_id": bot_user_id,
        "filter": filter
    }))
}

fn endpoint(base: &str) -> Result<Url, &'static str> {
    let url = Url::parse(base).map_err(|_| "matrix_stream_endpoint_invalid")?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err("matrix_stream_endpoint_invalid");
    }
    Ok(url)
}

pub(super) fn whoami_url(base: &str) -> Result<Url, &'static str> {
    let mut url = endpoint(base)?;
    url.set_path(&format!(
        "{}/_matrix/client/v3/account/whoami",
        url.path().trim_end_matches('/')
    ));
    Ok(url)
}

pub(super) fn verify_account(body: &Value, expected: &str) -> Result<(), &'static str> {
    if body.get("user_id").and_then(Value::as_str) != Some(expected)
        || body.get("errcode").is_some()
        || body.get("is_guest").is_some_and(|value| value.as_bool() != Some(false))
    {
        return Err("matrix_stream_account_unverified");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configured_zero_is_an_id_and_only_absence_disables_filter() {
        assert_eq!(configured_filter(Ok("0".into())), Ok(Some("0".into())));
        assert_eq!(configured_filter(Err(VarError::NotPresent)), Ok(None));
        for value in ["", " ", "\n"] {
            assert!(configured_filter(Ok(value.into())).is_err());
        }
        assert!(configured_filter(Err(VarError::NotUnicode("not-unicode-fixture".into()))).is_err());
        let raw = r#" { "room": {} } "#.to_string();
        assert_eq!(configured_filter(Ok(raw.clone())), Ok(Some(raw)));
    }

    #[test]
    fn endpoint_spelling_normalizes_but_filter_bytes_remain_exact() {
        let a = describe("https://MATRIX.example:443/proxy/", "@bot:example",
            Some(r#"{"room":{"timeline":{"limit":10}},"presence":{}}"#)).unwrap();
        let b = describe("https://matrix.example/proxy", "@bot:example",
            Some(r#"{ "presence": {}, "room": {"timeline": {"limit": 10}} }"#)).unwrap();
        assert_ne!(a, b);
        assert_eq!(a["homeserver"], b["homeserver"]);
        assert_eq!(describe("https://MATRIX.example:443/proxy/", "@bot:example", None).unwrap(),
            describe("https://matrix.example/proxy", "@bot:example", None).unwrap());
    }

    #[test]
    fn user_endpoint_and_filter_changes_are_distinct_streams() {
        let original = describe("https://m.example/proxy", "@bot:example", Some("0")).unwrap();
        for candidate in [
            describe("https://m.example/another", "@bot:example", Some("0")),
            describe("https://other.example/proxy", "@bot:example", Some("0")),
            describe("https://m.example/proxy", "@other:example", Some("0")),
            describe("https://m.example/proxy", "@bot:example", Some("1")),
            describe("https://m.example/proxy", "@bot:example", None),
            describe("https://m.example/proxy", "@bot:example", Some("{}")),
        ] {
            assert_ne!(original, candidate.unwrap());
        }
    }

    #[test]
    fn credentials_cannot_enter_scope_or_account_endpoint() {
        for base in ["file:///tmp/socket", "https://u:secret@m.example",
            "https://m.example?token=secret", "https://m.example#secret"] {
            assert!(describe(base, "@bot:example", None).is_err());
            assert!(whoami_url(base).is_err());
        }
    }

    #[test]
    fn invalid_filters_do_not_bind_a_new_partition() {
        for value in ["", " ", " id", "id\n", "{", "[]", "\"id\""] {
            assert!(describe("https://m.example", "@bot:example", Some(value)).is_err());
        }
        assert!(describe("https://m.example", "@bot:example", Some(&"x".repeat(4097))).is_err());
    }

    #[test]
    fn whoami_is_bound_to_configured_account_and_proxy_prefix() {
        let url = whoami_url("https://m.example/proxy/").unwrap();
        assert_eq!(url.as_str(), "https://m.example/proxy/_matrix/client/v3/account/whoami");
        assert_eq!(verify_account(&json!({"user_id":"@bot:example"}), "@bot:example"), Ok(()));
        assert_eq!(verify_account(&json!({"user_id":"@bot:example","is_guest":false}), "@bot:example"), Ok(()));
        for value in [json!({}), json!({"user_id":"@other:example"}),
            json!({"user_id":"@bot:example","is_guest":true}),
            json!({"user_id":"@bot:example","is_guest":null}),
            json!({"user_id":"@bot:example","errcode":"error"})] {
            assert!(verify_account(&value, "@bot:example").is_err());
        }
    }
}
