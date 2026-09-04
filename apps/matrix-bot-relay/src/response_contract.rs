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
        None => Ok(None),
        Some(reply) if reply.is_object()
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
    fn reply_shape_is_bounded_and_explicit() {
        for value in [json!({"projected_reply":null}), json!({"projected_reply":"text"}),
            json!({"projected_reply":{"msgtype":"m.text","body":" "}}),
            json!({"projected_reply":{"msgtype":"m.text","body":"x".repeat(65_537)}})] {
            assert!(bound_reply(&value, "!r:e").is_err());
        }
        let value = json!({"projected_reply":{"msgtype":"m.text","body":"bounded"}});
        assert!(bound_reply(&value, "!r:e").unwrap().is_some());
        assert_eq!(bound_reply(&json!({}), "!r:e"), Ok(None));
    }
}
