//! Unambiguous Matrix response envelopes, before cursor or admission decisions.
//!
//! Scan with Serde's own JSON decoder so escaped keys compare after decoding.
//! The second pass retains the existing target type's numeric/field semantics.
//! The caller still owns HTTP status, smaller endpoint limits and token checks.
use serde::de::{DeserializeOwned, DeserializeSeed, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer};
use std::{collections::HashSet, fmt};

const MAX_RESPONSE_BYTES: usize = 4_194_304;
const INVALID_RESPONSE: &str = "matrix_response_contract_invalid";

pub(super) fn decode<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, &'static str> {
    if bytes.len() > MAX_RESPONSE_BYTES {
        return Err("matrix_response_body_too_large");
    }
    let mut parser = serde_json::Deserializer::from_slice(bytes);
    CheckedJson { envelope: true }
        .deserialize(&mut parser)
        .map_err(|_| INVALID_RESPONSE)?;
    parser.end().map_err(|_| INVALID_RESPONSE)?;
    // Do not round-trip through Value: keep the original bytes and the target
    // deserializer's integer precision and strict field-type checks unchanged.
    serde_json::from_slice(bytes).map_err(|_| INVALID_RESPONSE)
}

/// Missing is allowed through #[serde(default)]; explicitly present null is not
/// a string and must not be mistaken for an exhausted pagination boundary.
pub(super) fn optional_string<'de, D>(parser: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    String::deserialize(parser).map(Some)
}

struct CheckedJson {
    envelope: bool,
}

impl<'de> DeserializeSeed<'de> for CheckedJson {
    type Value = ();

    fn deserialize<D>(self, parser: D) -> Result<(), D::Error>
    where
        D: Deserializer<'de>,
    {
        if self.envelope {
            parser.deserialize_map(self)
        } else {
            parser.deserialize_any(self)
        }
    }
}

impl<'de> Visitor<'de> for CheckedJson {
    type Value = ();

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a Matrix object without ambiguous keys")
    }

    fn visit_map<M>(self, mut map: M) -> Result<(), M::Error>
    where
        M: MapAccess<'de>,
    {
        let mut keys = HashSet::new();
        while let Some(key) = map.next_key::<String>()? {
            if (self.envelope && key == "errcode") || !keys.insert(key) {
                // No remote key or payload is copied into public diagnostics.
                return Err(serde::de::Error::custom(INVALID_RESPONSE));
            }
            map.next_value_seed(CheckedJson { envelope: false })?;
        }
        Ok(())
    }

    fn visit_seq<A>(self, mut sequence: A) -> Result<(), A::Error>
    where
        A: SeqAccess<'de>,
    {
        while sequence
            .next_element_seed(CheckedJson { envelope: false })?
            .is_some()
        {}
        Ok(())
    }

    fn visit_bool<E>(self, _value: bool) -> Result<(), E> {
        Ok(())
    }

    fn visit_i64<E>(self, _value: i64) -> Result<(), E> {
        Ok(())
    }

    fn visit_u64<E>(self, _value: u64) -> Result<(), E> {
        Ok(())
    }

    fn visit_f64<E>(self, _value: f64) -> Result<(), E> {
        Ok(())
    }

    fn visit_str<E>(self, _value: &str) -> Result<(), E> {
        Ok(())
    }

    fn visit_unit<E>(self) -> Result<(), E> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    #[derive(Debug, Deserialize, PartialEq)]
    struct Page {
        start: String,
        #[serde(default, deserialize_with = "optional_string")]
        end: Option<String>,
        chunk: Vec<Value>,
    }

    #[test]
    fn valid_envelope_preserves_values_and_integer_precision() {
        let raw = br#"{"u":18446744073709551615,"i":-9223372036854775808,"f":1.25,"b":true,"v":[false,null,"x",{}]}"#;
        assert_eq!(decode::<Value>(raw).unwrap(), serde_json::from_slice::<Value>(raw).unwrap());
    }

    #[test]
    fn duplicate_fields_cannot_select_a_different_cursor() {
        for raw in [
            br#"{"next_batch":"old","next_batch":"new"}"#.as_slice(),
            br#"{"next_batch":"old","next_\u0062atch":"new"}"#.as_slice(),
        ] {
            assert_eq!(decode::<Value>(raw).unwrap_err(), INVALID_RESPONSE);
        }
    }

    #[test]
    fn duplicate_room_maps_cannot_discard_an_earlier_timeline() {
        let raw = br#"{"rooms":{"join":{"!r:e":{"timeline":{"events":[1]}},"!r:e":{}}}}"#;
        assert!(decode::<Value>(raw).is_err());
    }

    #[test]
    fn nested_event_and_content_duplicates_are_rejected() {
        for raw in [
            br#"{"chunk":[{"sender":"@a:e","sender":"@b:e"}]}"#.as_slice(),
            br#"{"chunk":[{"content":{"body":"one","body":"two"}}]}"#.as_slice(),
            br#"{"extension":{"value":null,"value":1}}"#.as_slice(),
        ] {
            assert!(decode::<Value>(raw).is_err());
        }
    }

    #[test]
    fn repeated_keys_in_different_objects_are_not_duplicates() {
        let raw = br#"{"chunk":[{"type":"a"},{"type":"a"}],"type":"outer"}"#;
        assert!(decode::<Value>(raw).is_ok());
    }

    #[test]
    fn error_envelopes_cannot_be_success_even_with_a_cursor() {
        for errcode in [Value::Null, json!("M_UNKNOWN"), json!(false), json!({})] {
            let bytes = serde_json::to_vec(&json!({"next_batch":"new","errcode":errcode})).unwrap();
            assert_eq!(decode::<Value>(&bytes).unwrap_err(), INVALID_RESPONSE);
        }
        assert!(decode::<Value>(br#"{"err\u0063ode":"M_UNKNOWN","next_batch":"new"}"#).is_err());
    }

    #[test]
    fn nested_errcode_is_message_data_not_an_error_envelope() {
        assert!(decode::<Value>(br#"{"chunk":[{"content":{"errcode":"quoted text"}}]}"#).is_ok());
    }

    #[test]
    fn optional_end_distinguishes_absence_from_null() {
        let absent: Page = decode(br#"{"start":"a","chunk":[]}"#).unwrap();
        assert_eq!(absent.end, None);
        let present: Page = decode(br#"{"start":"a","end":"b","chunk":[]}"#).unwrap();
        assert_eq!(present.end.as_deref(), Some("b"));
        for raw in [
            br#"{"start":"a","end":null,"chunk":[]}"#.as_slice(),
            br#"{"start":"a","end":false,"chunk":[]}"#.as_slice(),
            br#"{"start":"a","end":0,"chunk":[]}"#.as_slice(),
        ] {
            assert!(decode::<Page>(raw).is_err());
        }
    }

    #[test]
    fn successful_prefix_cannot_hide_a_second_json_value() {
        for raw in [br#"{} {}"#.as_slice(), br#"{} null"#.as_slice(), br#"{}garbage"#.as_slice()] {
            assert!(decode::<Value>(raw).is_err());
        }
        assert!(decode::<Value>(b"{} \r\n\t").is_ok());
    }

    #[test]
    fn root_must_be_an_object() {
        for raw in [b"null".as_slice(), b"[]", b"1", b"true", br#""text""#] {
            assert!(decode::<Value>(raw).is_err());
        }
    }

    #[test]
    fn unknown_unique_extension_fields_are_preserved() {
        let raw = br#"{"start":"a","chunk":[],"org.example.future":{"enabled":true}}"#;
        assert!(decode::<Page>(raw).is_ok());
    }

    #[test]
    fn size_and_recursion_limits_remain_active() {
        let too_large = vec![b' '; MAX_RESPONSE_BYTES + 1];
        assert_eq!(decode::<Value>(&too_large).unwrap_err(), "matrix_response_body_too_large");
        let deep = format!("{{\"nested\":{}null{}}}", "[".repeat(256), "]".repeat(256));
        assert!(decode::<Value>(deep.as_bytes()).is_err());
    }

    #[test]
    fn invalid_utf8_and_truncated_objects_fail() {
        for raw in [b"{\"x\":\"\xff\"}".as_slice(), b"{\"x\":", b"{\"x\":\"\\uD800\"}"] {
            assert!(decode::<Value>(raw).is_err());
        }
    }
}
