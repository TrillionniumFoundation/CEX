//! Immutable sync-stream description; credential configuration is not read.
//! This describes configuration, not a proof of membership/history coverage.
use reqwest::Url;
use serde::{Deserialize, Deserializer, Serialize};
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
                validate_inline(raw)?;
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

// Missing selectors are optional; explicit JSON null is not a valid selector.
fn present_non_null<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer)?
        .map(Some)
        .ok_or_else(|| serde::de::Error::custom("matrix_filter_null_field"))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SyncFilter {
    #[serde(default, deserialize_with = "present_non_null")]
    room: Option<RoomFilter>,
    #[serde(default, deserialize_with = "present_non_null")]
    event_format: Option<String>,
    // These sections cannot select timeline events. They are not replayed here.
    #[serde(rename = "presence")]
    #[serde(default, deserialize_with = "present_non_null")]
    _presence: Option<Value>,
    #[serde(rename = "account_data")]
    #[serde(default, deserialize_with = "present_non_null")]
    _account_data: Option<Value>,
    // event_fields is intentionally unsupported: projecting away event identity
    // would cause normalisation to discard events while advancing the cursor.
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RoomFilter {
    #[serde(default, deserialize_with = "present_non_null")]
    rooms: Option<Vec<String>>,
    #[serde(default, deserialize_with = "present_non_null")]
    not_rooms: Option<Vec<String>>,
    #[serde(default, deserialize_with = "present_non_null")]
    include_leave: Option<bool>,
    #[serde(default, deserialize_with = "present_non_null")]
    timeline: Option<RoomEventFilter>,
    #[serde(rename = "state")]
    #[serde(default, deserialize_with = "present_non_null")]
    _state: Option<Value>,
    #[serde(rename = "ephemeral")]
    #[serde(default, deserialize_with = "present_non_null")]
    _ephemeral: Option<Value>,
    #[serde(rename = "account_data")]
    #[serde(default, deserialize_with = "present_non_null")]
    _account_data: Option<Value>,
}

#[derive(Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct RoomEventFilter {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default, deserialize_with = "present_non_null")]
    rooms: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default, deserialize_with = "present_non_null")]
    not_rooms: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default, deserialize_with = "present_non_null")]
    senders: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default, deserialize_with = "present_non_null")]
    not_senders: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default, deserialize_with = "present_non_null")]
    types: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default, deserialize_with = "present_non_null")]
    not_types: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default, deserialize_with = "present_non_null")]
    contains_url: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default, deserialize_with = "present_non_null")]
    limit: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default, deserialize_with = "present_non_null")]
    lazy_load_members: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default, deserialize_with = "present_non_null")]
    include_redundant_members: Option<bool>,
}

fn parse_inline(raw: &str) -> Result<SyncFilter, &'static str> {
    // Matrix discriminates inline filters by the FIRST character, not trim().
    if !raw.starts_with('{') || raw.len() > 4096 {
        return Err("matrix_sync_filter_unsupported");
    }
    let filter: SyncFilter =
        serde_json::from_str(raw).map_err(|_| "matrix_sync_filter_unsupported")?;
    if filter
        .event_format
        .as_deref()
        .is_some_and(|format| format != "client")
        || filter
            .room
            .as_ref()
            .is_some_and(|room| room.include_leave == Some(true))
    {
        return Err("matrix_sync_filter_unsupported");
    }
    Ok(filter)
}

pub(super) fn validate_inline(raw: &str) -> Result<(), &'static str> {
    parse_inline(raw).map(|_| ())
}

fn room_allowed(room: &str, allowed: &Option<Vec<String>>, denied: &Option<Vec<String>>) -> bool {
    allowed
        .as_ref()
        .is_none_or(|rooms| rooms.iter().any(|candidate| candidate == room))
        && !denied
            .as_ref()
            .is_some_and(|rooms| rooms.iter().any(|candidate| candidate == room))
}

pub(super) fn backfill_filter(
    raw: Option<&str>,
    room_id: &str,
) -> Result<Option<String>, &'static str> {
    let Some(raw) = raw else {
        return Ok(None);
    };
    // Server-side IDs require a pinned definition. Until that protocol exists,
    // hold this gap instead of silently fetching unfiltered historical commands.
    if !raw.starts_with('{') {
        return Err("matrix_gap_filter_id_requires_resolution");
    }
    let filter = parse_inline(raw)?;
    let Some(room) = filter.room else {
        return Ok(None);
    };
    if !room_allowed(room_id, &room.rooms, &room.not_rooms) {
        return Err("matrix_gap_room_excluded_by_filter");
    }
    let Some(timeline) = room.timeline else {
        return Ok(None);
    };
    if !room_allowed(room_id, &timeline.rooms, &timeline.not_rooms) {
        return Err("matrix_gap_room_excluded_by_filter");
    }
    // Retain limits and every supported timeline predicate. Do not copy state,
    // account-data, presence or top-level room selectors as a RoomEventFilter.
    serde_json::to_string(&timeline)
        .map(Some)
        .map_err(|_| "matrix_sync_filter_unsupported")
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
        assert_eq!(configured_filter(Ok(raw.clone())), Ok(Some(raw.clone())));
        assert!(describe("https://m.example", "@bot:example", Some(&raw)).is_err());
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

    #[test]
    fn no_filter_and_no_timeline_predicate_do_not_invent_restrictions() {
        assert_eq!(backfill_filter(None, "!r:e"), Ok(None));
        assert_eq!(backfill_filter(Some("{}"), "!r:e"), Ok(None));
        assert_eq!(backfill_filter(Some(r#"{"room":{"rooms":["!r:e"]}}"#), "!r:e"), Ok(None));
    }

    #[test]
    fn timeline_membership_predicates_are_preserved() {
        let timeline = json!({"senders":["@allowed:e"],"not_senders":["@denied:e"],
            "types":["m.room.message"],"not_types":["m.room.encrypted"],
            "contains_url":false,"rooms":["!r:e"],"not_rooms":["!other:e"],
            "limit":10,"lazy_load_members":true,"include_redundant_members":false});
        let source = json!({"event_format":"client", "presence":{"limit":0},
            "room":{"rooms":["!r:e"],"timeline":timeline.clone(),"state":{"types":[]}}});
        let actual = backfill_filter(Some(&source.to_string()), "!r:e").unwrap().unwrap();
        assert_eq!(serde_json::from_str::<Value>(&actual).unwrap(), timeline);
    }

    #[test]
    fn room_exclusions_cannot_be_ignored_during_backfill() {
        for raw in [r#"{"room":{"rooms":[]}}"#,
            r#"{"room":{"rooms":["!other:e"]}}"#,
            r#"{"room":{"rooms":["!r:e"],"not_rooms":["!r:e"]}}"#,
            r#"{"room":{"timeline":{"rooms":[]}}}"#,
            r#"{"room":{"timeline":{"not_rooms":["!r:e"]}}}"#] {
            assert_eq!(backfill_filter(Some(raw), "!r:e"), Err("matrix_gap_room_excluded_by_filter"));
        }
    }

    #[test]
    fn ids_cannot_turn_into_unfiltered_backfill() {
        for id in ["0", "opaque-id", "42"] {
            assert_eq!(backfill_filter(Some(id), "!r:e"), Err("matrix_gap_filter_id_requires_resolution"));
        }
    }

    #[test]
    fn leading_space_is_not_an_inline_filter() {
        for raw in [" {}", "\t{}", "\n{}"] {
            assert!(validate_inline(raw).is_err());
        }
        assert!(validate_inline("{} \n").is_ok());
    }

    #[test]
    fn unsupported_projection_and_extensions_fail_closed() {
        for raw in [r#"{"event_fields":["content"]}"#,
            r#"{"event_fields":null}"#,
            r#"{"event_format":"federation"}"#,
            r#"{"room":{"include_leave":true}}"#,
            r#"{"room":{"timeline":{"org.example.filter":true}}}"#,
            r#"{"org.example.filter":{}}"#] {
            assert!(validate_inline(raw).is_err());
        }
    }

    #[test]
    fn duplicate_selectors_and_invalid_types_are_not_reinterpreted() {
        for raw in [r#"{"room":{},"room":{"rooms":[]}}"#,
            r#"{"room":{"timeline":{"senders":[],"senders":["@a:e"]}}}"#,
            r#"{"room":{"timeline":{"limit":-1}}}"#,
            r#"{"room":{"timeline":{"contains_url":"false"}}}"#,
            r#"{"room":{"rooms":[1]}}"#] {
            assert!(validate_inline(raw).is_err());
        }
    }
}
