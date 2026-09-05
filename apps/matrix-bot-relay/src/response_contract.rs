//! Transport evidence validation. A 2xx response is not a Matrix send receipt.
use serde_json::Value;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum BodyFailure {
    TooLarge,
    Interrupted,
}

#[derive(Debug)]
pub(super) enum MatrixDecision {
    Accepted(Value),
    Retry(&'static str),
    Hold(&'static str),
}

pub(super) fn classify_matrix_response(status: u16, body: Result<Vec<u8>, BodyFailure>) -> MatrixDecision {
    let bytes = match body {
        Ok(bytes) => bytes,
        Err(BodyFailure::Interrupted) => return MatrixDecision::Retry("matrix_response_unknown_interrupted"),
        Err(BodyFailure::TooLarge) => return MatrixDecision::Hold("matrix_unverified_oversized_response"),
    };
    if status == 200 {
        let receipt = match serde_json::from_slice::<Value>(&bytes) {
            Ok(value) => value,
            Err(_) => return MatrixDecision::Retry("matrix_response_unknown_invalid_json"),
        };
        let valid = receipt.get("event_id").and_then(Value::as_str).is_some_and(|id| {
            id.starts_with('$') && id.len() > 1 && id.len() <= 512 && !id.chars().any(char::is_whitespace)
                && !id.chars().any(char::is_control)
        });
        if valid && receipt.get("errcode").is_none() {
            return MatrixDecision::Accepted(receipt);
        }
        return MatrixDecision::Retry("matrix_response_unknown_missing_receipt");
    }
    if status == 408 || status == 429 || (500..=599).contains(&status) || (200..=299).contains(&status) {
        return MatrixDecision::Retry("matrix_response_unknown_status");
    }
    MatrixDecision::Hold("matrix_send_rejected_status")
}

/// Verify the actual MatrixAdapterResponse envelope, not just HTTP success.
/// The legacy status handler uses the requested task ID as its event_id; that
/// exception is accepted only for the exact /status command in the source bytes.
/// A duplicate-cache hit cannot prove that the original business action finished.
pub(super) fn validate_adapter_response(
    upstream: &Value,
    source: &Value,
) -> Result<(), &'static str> {
    let accepted = upstream
        .get("accepted")
        .and_then(Value::as_bool)
        .ok_or("adapter_response_missing_disposition")?;
    let action = upstream
        .get("action")
        .and_then(Value::as_str)
        .filter(|value| {
            !value.is_empty()
                && value.len() <= 128
                && value.bytes().all(|byte| byte.is_ascii_lowercase() || byte == b'_')
        })
        .ok_or("adapter_response_invalid_action")?;
    if upstream.get("error").is_some() || upstream.get("errcode").is_some() {
        return Err("adapter_response_error_envelope");
    }
    for field in ["room_id", "sender"] {
        let expected = source
            .get(field)
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .ok_or("adapter_source_identity_missing")?;
        if upstream.get(field).and_then(Value::as_str) != Some(expected) {
            return Err("adapter_response_identity_mismatch");
        }
    }
    let source_event = source
        .get("event_id")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or("adapter_source_identity_missing")?;
    let expected_event = if action == "status_lookup" {
        let text = source.get("text").and_then(Value::as_str).or_else(|| {
            source.get("content").and_then(|content| content.get("body")).and_then(Value::as_str)
        }).ok_or("adapter_status_request_mismatch")?;
        let mut words = text.split_whitespace();
        if words.next() != Some("/status") {
            return Err("adapter_status_request_mismatch");
        }
        words.next().ok_or("adapter_status_request_mismatch")?
    } else {
        source_event
    };
    if upstream.get("event_id").and_then(Value::as_str) != Some(expected_event) {
        return Err("adapter_response_event_mismatch");
    }
    if action == "duplicate_event" {
        return Err("adapter_duplicate_outcome_unknown");
    }
    let non_business_action = matches!(
        action,
        "help" | "unsupported_command" | "ignored_self_event" | "ignored_non_text_event"
    );
    if accepted == non_business_action {
        return Err("adapter_response_disposition_mismatch");
    }
    Ok(())
}

pub(super) fn bound_reply(upstream: &Value, original_room: &str) -> Result<Option<Value>, &'static str> {
    if !upstream.is_object() {
        return Err("adapter_response_unknown_shape");
    }
    if let Some(room) = upstream.get("room_id") {
        if room.as_str() != Some(original_room) {
            return Err("adapter_reply_room_mismatch");
        }
    }
    match upstream.get("projected_reply") {
        None | Some(Value::Null) => Ok(None),
        Some(reply) if reply.as_object().is_some_and(|object| {
            object.keys().all(|key| matches!(key.as_str(), "msgtype" | "body"))
        })
            && reply.get("msgtype").and_then(Value::as_str).is_some_and(|kind| matches!(kind, "m.text" | "m.notice"))
            && reply.get("body").and_then(Value::as_str).is_some_and(|body| {
                !body.trim().is_empty() && body.len() <= 65_536
            }) => Ok(Some(reply.clone())),
        Some(_) => Err("adapter_reply_contract_mismatch"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn send_requires_a_real_event_id() {
        for body in [r#"{}"#, r#"{"event_id":""}"#, r#"{"event_id":"not-an-event"}"#, r#"{"event_id":"$x","errcode":"bad"}"#] {
            assert!(matches!(classify_matrix_response(200, Ok(body.as_bytes().to_vec())), MatrixDecision::Retry(_)));
        }
        assert!(matches!(classify_matrix_response(200, Ok(br#"{"event_id":"$valid"}"#.to_vec())), MatrixDecision::Accepted(_)));
    }

    #[test]
    fn malformed_and_interrupted_responses_are_unknown_not_success() {
        assert!(matches!(classify_matrix_response(200, Ok(b"{".to_vec())), MatrixDecision::Retry(_)));
        assert!(matches!(classify_matrix_response(200, Err(BodyFailure::Interrupted)), MatrixDecision::Retry(_)));
        assert!(matches!(classify_matrix_response(200, Err(BodyFailure::TooLarge)), MatrixDecision::Hold(_)));
    }

    #[test]
    fn status_alone_never_completes_a_send() {
        for status in [201, 202, 204, 408, 429, 500, 503] {
            assert!(matches!(classify_matrix_response(status, Ok(Vec::new())), MatrixDecision::Retry(_)));
        }
        for status in [301, 400, 401, 403, 404] {
            assert!(matches!(classify_matrix_response(status, Ok(Vec::new())), MatrixDecision::Hold(_)));
        }
    }

    #[test]
    fn replies_cannot_target_another_room() {
        let reply = json!({"room_id":"!other:e", "projected_reply":{"msgtype":"m.text","body":"private"}});
        assert_eq!(bound_reply(&reply, "!original:e"), Err("adapter_reply_room_mismatch"));
        let reply = json!({"room_id": null});
        assert_eq!(bound_reply(&reply, "!original:e"), Err("adapter_reply_room_mismatch"));
    }

    #[test]
    fn presentation_reply_cannot_carry_edit_html_or_mention_control_fields() {
        for field in ["m.relates_to", "m.new_content", "m.mentions", "format", "formatted_body", "url"] {
            let mut reply = json!({"msgtype":"m.text", "body":"bounded"});
            reply[field] = json!({"unexpected":true});
            assert_eq!(bound_reply(&json!({"projected_reply":reply}), "!r:e"),
                Err("adapter_reply_contract_mismatch"));
        }
    }

    #[test]
    fn reply_shape_is_bounded_and_explicit() {
        for value in [json!({"projected_reply":42}), json!({"projected_reply":"text"}),
            json!({"projected_reply":{"msgtype":"m.text","body":" "}}),
            json!({"projected_reply":{"msgtype":"m.text","body":"x".repeat(65_537)}})] {
            assert!(bound_reply(&value, "!r:e").is_err());
        }
        let value = json!({"projected_reply":{"msgtype":"m.text","body":"bounded"}});
        assert!(bound_reply(&value, "!r:e").unwrap().is_some());
        assert_eq!(bound_reply(&json!({}), "!r:e"), Ok(None));
    }
}

#[cfg(test)]
mod adapter_tests {
    use super::{bound_reply, validate_adapter_response};
    use serde_json::{json, Value};

    fn source() -> Value {
        json!({"event_id":"$source", "room_id":"!room:example", "sender":"@human:example", "text":"hello"})
    }

    fn response() -> Value {
        json!({"accepted":true, "action":"forwarded_to_consumer_entry", "event_id":"$source",
               "room_id":"!room:example", "sender":"@human:example", "forwarded":{}, "projected_reply":null})
    }

    #[test]
    fn valid_envelope_and_explicit_null_reply_are_compatible() {
        let value = response();
        assert_eq!(validate_adapter_response(&value, &source()), Ok(()));
        assert_eq!(bound_reply(&value, "!room:example"), Ok(None));
    }

    #[test]
    fn empty_or_error_envelopes_are_not_acceptance() {
        for value in [json!({}), json!(null), json!([])] {
            assert!(validate_adapter_response(&value, &source()).is_err());
        }
        let mut value = response();
        value["error"] = json!("not accepted");
        assert!(validate_adapter_response(&value, &source()).is_err());
    }

    #[test]
    fn every_response_identity_is_bound_to_the_source() {
        for field in ["event_id", "room_id", "sender"] {
            let mut value = response();
            value[field] = json!("another-identity");
            assert!(validate_adapter_response(&value, &source()).is_err());
            value[field] = Value::Null;
            assert!(validate_adapter_response(&value, &source()).is_err());
        }
    }

    #[test]
    fn help_can_reply_without_claiming_a_business_effect() {
        let mut value = response();
        value["action"] = json!("help");
        value["accepted"] = json!(false);
        value["projected_reply"] = json!({"msgtype":"m.text","body":"help"});
        assert_eq!(validate_adapter_response(&value, &source()), Ok(()));
        assert!(bound_reply(&value, "!room:example").unwrap().is_some());
    }

    #[test]
    fn duplicate_cache_hit_is_unknown_not_a_completed_result() {
        let mut value = response();
        value["action"] = json!("duplicate_event");
        value["accepted"] = json!(false);
        assert_eq!(validate_adapter_response(&value, &source()), Err("adapter_duplicate_outcome_unknown"));
    }

    #[test]
    fn status_response_binds_the_requested_task_not_an_arbitrary_task() {
        let mut original = source();
        original["text"] = json!("/status task-123");
        let mut value = response();
        value["action"] = json!("status_lookup");
        value["event_id"] = json!("task-123");
        assert_eq!(validate_adapter_response(&value, &original), Ok(()));
        assert!(validate_adapter_response(&value, &source()).is_err());
        value["event_id"] = json!("task-456");
        assert!(validate_adapter_response(&value, &original).is_err());
    }

    #[test]
    fn accepted_false_unknown_actions_and_bad_types_do_not_complete() {
        let mut value = response();
        value["accepted"] = json!(false);
        assert!(validate_adapter_response(&value, &source()).is_err());
        value["accepted"] = json!("true");
        assert!(validate_adapter_response(&value, &source()).is_err());
        value["accepted"] = json!(true);
        value["action"] = json!("invalid\nprivate");
        assert!(validate_adapter_response(&value, &source()).is_err());
    }
}
