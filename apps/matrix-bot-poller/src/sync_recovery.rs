//! Bounded Matrix pagination mechanics. Tokens remain opaque; a short or empty
//! page is not a completion signal when an `end` token is present.
use anyhow::{anyhow, bail, Result};
use reqwest::Url;
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashSet;

#[derive(Debug, Deserialize)]
pub(super) struct MessagePage {
    pub start: String,
    #[serde(default, deserialize_with = "super::wire_response::optional_string")]
    pub end: Option<String>,
    pub chunk: Vec<Value>,
}

pub(super) struct GapPager {
    cursor: String,
    stop: String,
    seen: HashSet<String>,
    complete: bool,
}

impl GapPager {
    pub fn new(from: &str, stop: &str) -> Result<Self> {
        validate_token(from)?;
        validate_token(stop)?;
        Ok(Self {
            cursor: from.to_string(),
            stop: stop.to_string(),
            seen: HashSet::from([from.to_string()]),
            complete: from == stop,
        })
    }

    pub fn cursor(&self) -> &str {
        &self.cursor
    }

    pub fn complete(&self) -> bool {
        self.complete
    }

    pub fn accept(&mut self, page: &MessagePage, limit: usize) -> Result<()> {
        if self.complete || page.start != self.cursor || page.chunk.len() > limit {
            bail!("matrix_gap_page_contract_mismatch");
        }
        match page.end.as_deref() {
            None => self.complete = true,
            Some(next) => {
                validate_token(next)?;
                if next == self.stop {
                    self.complete = true;
                } else if !self.seen.insert(next.to_string()) {
                    bail!("matrix_gap_cursor_cycle");
                }
                self.cursor = next.to_string();
            }
        }
        Ok(())
    }
}

pub(super) fn validate_token(value: &str) -> Result<()> {
    if value.is_empty() || value.len() > 8192 || value.chars().any(char::is_control) {
        bail!("invalid_matrix_pagination_token");
    }
    Ok(())
}

pub(super) fn messages_url(base: &str, room: &str, from: &str, to: &str, limit: usize, filter: Option<&str>) -> Result<Url> {
    if room.is_empty() || room.len() > 512 || room.chars().any(char::is_control) {
        bail!("invalid_matrix_gap_room");
    }
    validate_token(from)?;
    validate_token(to)?;
    let mut url = Url::parse(base).map_err(|_| anyhow!("invalid_matrix_gap_endpoint"))?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none()
        || !url.username().is_empty() || url.password().is_some()
        || url.query().is_some() || url.fragment().is_some()
    {
        bail!("invalid_matrix_gap_endpoint");
    }
    url.path_segments_mut().map_err(|_| anyhow!("invalid_matrix_gap_endpoint"))?
        .pop_if_empty().extend(["_matrix", "client", "v3", "rooms", room, "messages"]);
    url.query_pairs_mut().append_pair("dir", "b").append_pair("from", from)
        .append_pair("to", to).append_pair("limit", &limit.to_string());
    if let Some(filter) = filter {
        if filter.len() > 4096 || !filter.starts_with('{') {
            bail!("matrix_gap_filter_invalid");
        }
        url.query_pairs_mut().append_pair("filter", filter);
    }
    Ok(url)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn page(start: &str, end: Option<&str>, chunk: Vec<Value>) -> MessagePage {
        MessagePage { start: start.into(), end: end.map(str::to_string), chunk }
    }

    #[test]
    fn empty_page_with_next_cursor_must_continue() {
        let mut pager = GapPager::new("new", "old").unwrap();
        pager.accept(&page("new", Some("middle"), vec![]), 100).unwrap();
        assert!(!pager.complete());
        assert_eq!(pager.cursor(), "middle");
    }

    #[test]
    fn only_absent_end_or_exact_stop_completes() {
        for end in [None, Some("old")] {
            let mut pager = GapPager::new("new", "old").unwrap();
            pager.accept(&page("new", end, vec![json!({"event_id":"e"})]), 100).unwrap();
            assert!(pager.complete());
        }
    }

    #[test]
    fn same_boundary_requires_no_fetch() {
        assert!(GapPager::new("same", "same").unwrap().complete());
    }

    #[test]
    fn immediate_and_multi_page_cycles_fail_closed() {
        let mut pager = GapPager::new("a", "old").unwrap();
        assert!(pager.accept(&page("a", Some("a"), vec![]), 100).is_err());
        let mut pager = GapPager::new("a", "old").unwrap();
        pager.accept(&page("a", Some("b"), vec![]), 100).unwrap();
        assert!(pager.accept(&page("b", Some("a"), vec![]), 100).is_err());
    }

    #[test]
    fn wrong_start_and_oversized_page_are_rejected() {
        let mut pager = GapPager::new("a", "old").unwrap();
        assert!(pager.accept(&page("wrong", None, vec![]), 100).is_err());
        assert!(pager.accept(&page("a", None, vec![json!(1), json!(2)]), 1).is_err());
    }

    #[test]
    fn missing_required_page_fields_and_empty_tokens_fail() {
        assert!(serde_json::from_str::<MessagePage>(r#"{"chunk":[]}"#).is_err());
        assert!(serde_json::from_str::<MessagePage>(r#"{"start":"a"}"#).is_err());
        let mut pager = GapPager::new("a", "old").unwrap();
        assert!(pager.accept(&page("a", Some(""), vec![]), 100).is_err());
        assert!(GapPager::new("a\nsecret", "old").is_err());
    }

    #[test]
    fn url_encodes_room_and_query_without_authority_change() {
        let url = messages_url("https://matrix.example/proxy", "!r/?:example", "a&x=1", "old", 100, None).unwrap();
        assert_eq!(url.host_str(), Some("matrix.example"));
        assert!(url.path().starts_with("/proxy/_matrix/client/v3/rooms/"));
        assert_eq!(url.query_pairs().find(|(k, _)| k == "from").unwrap().1, "a&x=1");
        assert!(!url.query_pairs().any(|(k, _)| k == "x"));
    }

    #[test]
    fn endpoint_cannot_contain_credentials_or_query() {
        for base in ["https://user:secret@matrix.example", "https://matrix.example?token=secret", "file:///tmp/x"] {
            assert!(messages_url(base, "!r:e", "a", "b", 100, None).is_err());
        }
    }
    #[test]
    fn message_filter_is_one_encoded_query_value() {
        let filter = r#"{"senders":["@a&b:e"],"contains_url":false}"#;
        let url = messages_url("https://matrix.example/proxy", "!r:e", "new", "old", 100, Some(filter)).unwrap();
        assert_eq!(url.query_pairs().find(|(key, _)| key == "filter").unwrap().1, filter);
        assert_eq!(url.query_pairs().filter(|(key, _)| key == "filter").count(), 1);
        assert_eq!(url.query_pairs().count(), 5);
    }

    #[test]
    fn message_filter_ids_cannot_be_sent_as_json_filters() {
        for filter in ["0", " {}", ""] {
            assert!(messages_url("https://matrix.example", "!r:e", "new", "old", 100, Some(filter)).is_err());
        }
    }

    #[test]
    fn null_end_is_not_a_terminal_page() {
        let bytes = br#"{"start":"new","end":null,"chunk":[]}"#;
        assert!(super::super::wire_response::decode::<MessagePage>(bytes).is_err());
        // The field-level rule also protects direct typed deserialization.
        assert!(serde_json::from_slice::<MessagePage>(bytes).is_err());
    }

    #[test]
    fn omitted_end_and_empty_continuation_keep_distinct_meanings() {
        let terminal: MessagePage = super::super::wire_response::decode(
            br#"{"start":"new","chunk":[]}"#,
        ).unwrap();
        let mut pager = GapPager::new("new", "old").unwrap();
        pager.accept(&terminal, 100).unwrap();
        assert!(pager.complete());

        let continuation: MessagePage = super::super::wire_response::decode(
            br#"{"start":"new","end":"middle","chunk":[]}"#,
        ).unwrap();
        let mut pager = GapPager::new("new", "old").unwrap();
        pager.accept(&continuation, 100).unwrap();
        assert!(!pager.complete());
        assert_eq!(pager.cursor(), "middle");
    }

    #[test]
    fn ambiguous_and_error_pages_never_reach_pagination() {
        for bytes in [
            br#"{"start":"new","chunk":[],"errcode":"M_UNKNOWN"}"#.as_slice(),
            br#"{"start":"new","end":null,"end":"old","chunk":[]}"#.as_slice(),
            br#"{"start":"new","chunk":[{"type":"m.room.message","type":"m.room.member"}]}"#.as_slice(),
        ] {
            assert!(super::super::wire_response::decode::<MessagePage>(bytes).is_err());
        }
    }

}
