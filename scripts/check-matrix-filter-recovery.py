#!/usr/bin/env python3
"""Check exact function-local filter wiring. This is not Rust execution proof."""
from __future__ import annotations

import json
from pathlib import Path
from rust_route_contract import delimiter_pairs, tokenize

ROOT = Path(__file__).resolve().parents[1]
PATHS = {
    'poller': 'apps/matrix-bot-poller/src/main.rs',
    'policy': 'apps/matrix-bot-poller/src/stream_scope.rs',
    'scope': 'apps/matrix-bot-poller/src/stream_scope.rs',
    'recovery': 'apps/matrix-bot-poller/src/sync_recovery.rs',
}


def function_body(source: str, name: str) -> str:
    """Use balanced Rust tokens; comments, string fixtures and other functions do not count."""
    tokens = tokenize(source)
    pairs = delimiter_pairs(tokens)
    matches = []
    for i in range(len(tokens) - 2):
        if tokens[i].kind != 'ident' or tokens[i].text != 'fn' or tokens[i + 1].text != name:
            continue
        j = i + 2
        while j < len(tokens) and tokens[j].text not in {'{', ';'}:
            j = pairs[j] + 1 if j in pairs else j + 1
        if j < len(tokens) and tokens[j].text == '{':
            matches.append(source[tokens[j].end:tokens[pairs[j]].offset])
    if len(matches) != 1:
        raise AssertionError(f'expected one function definition: {name}')
    return matches[0]


def require(source: str, *markers: str) -> None:
    for marker in markers:
        if marker not in source:
            raise AssertionError('missing filter source contract: ' + marker)


def validate(sources: dict[str, str]) -> None:
    policy = sources['policy']
    parse = function_body(policy, 'parse_inline')
    require(parse, "!raw.starts_with('{')", 'raw.len() > 4096', 'serde_json::from_str(raw)',
            'format != "client"', 'room.include_leave == Some(true)')
    # The type-bound strict schema, not a marker in tests or prose.
    start = policy.index('struct SyncFilter {')
    end = policy.index('\n}', start)
    sync_fields = policy[start:end]
    require(policy[:start], '#[serde(deny_unknown_fields)]')
    if 'event_fields:' in sync_fields:
        raise AssertionError('event identity projection is not supported')
    for field in ('senders', 'not_senders', 'types', 'not_types', 'contains_url', 'rooms', 'not_rooms'):
        require(policy, field + ': Option<')
    project = function_body(policy, 'backfill_filter')
    require(project, 'matrix_gap_filter_id_requires_resolution', 'parse_inline(raw)?',
            'room_allowed(room_id, &room.rooms, &room.not_rooms)',
            'room_allowed(room_id, &timeline.rooms, &timeline.not_rooms)',
            'serde_json::to_string(&timeline)', '.map(Some)')
    allow = function_body(policy, 'room_allowed')
    require(allow, 'candidate == room', '&& !denied', '.is_none_or(')
    scope = function_body(sources['scope'], 'describe')
    require(scope, 'validate_inline(raw)?')
    recover = function_body(sources['poller'], 'recover_limited_timelines')
    markers = ['filter_definition::effective_filter(', 'stream_scope::backfill_filter(effective, room_id)', 'renew_cursor_lease(', 'sync_recovery::messages_url(',
               'message_filter.as_deref()', 'http.get(url)']
    position = -1
    for marker in markers:
        position = recover.find(marker, position + 1)
        if position < 0:
            raise AssertionError('filter not enforced before recovery I/O: ' + marker)
    urls = function_body(sources['recovery'], 'messages_url')
    require(urls, 'if let Some(filter) = filter', 'filter.len() > 4096',
            'url.query_pairs_mut().append_pair("filter", filter)')
    if 'unwrap_or_default' in project or 'unwrap_or_default' in recover:
        raise AssertionError('filter errors cannot become unfiltered defaults')


def sources() -> dict[str, str]:
    return {key: (ROOT / relative).read_text(encoding='utf-8') for key, relative in PATHS.items()}


def main() -> int:
    try:
        validate(sources())
    except (AssertionError, OSError, ValueError) as error:
        print('Matrix filter recovery source FAILED: ' + str(error))
        return 1
    print(json.dumps({'schema': 'cex.matrix-filter-recovery-source.v1', 'status': 'ok',
                      'rust_executed': False, 'production_authorization': 'not_granted'}))
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
