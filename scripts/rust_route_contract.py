#!/usr/bin/env python3
"""Bounded lexical extraction of explicit Rust/Axum route declarations.

This is not type resolution, macro expansion, cfg evaluation or a runtime router.
Only the outer MethodRouter construction is interpreted. Handler bodies, comments,
strings and neighbouring declarations cannot contribute methods to that router.
"""
from __future__ import annotations

from dataclasses import dataclass
import hashlib
import re

MAX_BYTES = 16 * 1024 * 1024
MAX_TOKENS = 750_000
MAX_NESTING = 256
METHODS = frozenset({'GET', 'POST', 'PUT', 'PATCH', 'DELETE', 'HEAD', 'OPTIONS', 'TRACE', 'CONNECT'})
REGISTRATIONS = frozenset({'route', 'route_service', 'nest', 'nest_service'})
TRANSPARENT_METHODS = frozenset({'layer', 'route_layer', 'with_state', 'handle_error'})
IDENTIFIER_RE = re.compile(r'(?:r#)?[A-Za-z_][A-Za-z_0-9]*')


class RouteSyntaxError(ValueError):
    """A source structure could not be parsed safely; diagnostics omit literals."""


@dataclass(frozen=True)
class Token:
    text: str
    kind: str
    offset: int
    end: int


@dataclass(frozen=True)
class RouteDeclaration:
    value: str | None
    registration: str
    methods: tuple[str, ...]
    observed_methods: tuple[str, ...]
    path_resolution: str
    method_resolution: str
    offset: int
    end: int
    expression_sha256: str


def _error(source: str, offset: int, reason: str) -> RouteSyntaxError:
    return RouteSyntaxError(f'{reason} at source line {source.count(chr(10), 0, offset) + 1}')


def tokenize(source: str) -> list[Token]:
    if len(source.encode('utf-8')) > MAX_BYTES:
        raise RouteSyntaxError('Rust source exceeds the lexical byte budget')
    tokens: list[Token] = []
    i, n = 0, len(source)
    while i < n:
        if len(tokens) >= MAX_TOKENS:
            raise RouteSyntaxError('Rust source exceeds the lexical token budget')
        if source[i].isspace():
            i += 1
            continue
        if source.startswith('//', i):
            end = source.find('\n', i + 2)
            i = n if end < 0 else end + 1
            continue
        if source.startswith('/*', i):
            start, depth = i, 1
            i += 2
            while i < n and depth:
                if source.startswith('/*', i):
                    depth += 1
                    if depth > MAX_NESTING:
                        raise _error(source, start, 'block comment nesting exceeded')
                    i += 2
                elif source.startswith('*/', i):
                    depth -= 1
                    i += 2
                else:
                    i += 1
            if depth:
                raise _error(source, start, 'unterminated block comment')
            continue
        raw = re.match(r'(?:br|cr|r)(#{0,255})"', source[i:i + 260])
        if raw:
            start = i
            terminator = '"' + raw.group(1)
            end = source.find(terminator, i + raw.end())
            if end < 0:
                raise _error(source, i, 'unterminated raw string')
            i = end + len(terminator)
            kind = 'raw_string' if source[start] == 'r' else 'other_literal'
            tokens.append(Token(source[start:i], kind, start, i))
            continue
        quote = i if source[i] == '"' else (i + 1 if source.startswith(('b"', 'c"'), i) else -1)
        if quote >= 0:
            start, i = i, quote + 1
            while i < n:
                if source[i] == '\\':
                    i += 2
                elif source[i] == '"':
                    i += 1
                    break
                else:
                    i += 1
            else:
                raise _error(source, start, 'unterminated string')
            tokens.append(Token(source[start:i], 'string' if start == quote else 'other_literal', start, i))
            continue
        # Character literals must not disturb delimiter matching. A Rust lifetime
        # such as 'a is not a character literal and is kept as ordinary tokens.
        char = re.match(r"(?:b)?'(?:\\u\{[0-9A-Fa-f_]+\}|\\x[0-9A-Fa-f]{2}|\\[^\r\n]|[^'\\\r\n])'", source[i:i + 32])
        if char:
            end = i + char.end()
            tokens.append(Token(source[i:end], 'other_literal', i, end))
            i = end
            continue
        identifier = IDENTIFIER_RE.match(source, i)
        if identifier:
            end = identifier.end()
            tokens.append(Token(source[i:end], 'ident', i, end))
            i = end
            continue
        tokens.append(Token(source[i], 'punct', i, i + 1))
        i += 1
    return tokens


def delimiter_pairs(tokens: list[Token]) -> dict[int, int]:
    stack: list[int] = []
    pairs: dict[int, int] = {}
    opening = {'(': ')', '[': ']', '{': '}'}
    closing = frozenset(opening.values())
    for index, token in enumerate(tokens):
        if token.kind != 'punct':
            continue
        if token.text in opening:
            stack.append(index)
            if len(stack) > MAX_NESTING:
                raise RouteSyntaxError('Rust delimiter nesting budget exceeded')
        elif token.text in closing:
            if not stack or opening[tokens[stack[-1]].text] != token.text:
                raise RouteSyntaxError('mismatched Rust source delimiters')
            start = stack.pop()
            pairs[start] = index
    if stack:
        raise RouteSyntaxError('unclosed Rust source delimiter')
    return pairs


def _arguments(tokens: list[Token], pairs: dict[int, int], lo: int, hi: int) -> list[tuple[int, int]]:
    args, start, i = [], lo, lo
    while i < hi:
        if tokens[i].text == ',' and tokens[i].kind == 'punct':
            args.append((start, i))
            start = i + 1
        if tokens[i].text == '<' and i >= lo + 2 and tokens[i - 1].text == ':' and tokens[i - 2].text == ':':
            depth = 1
            i += 1
            while i < hi and depth:
                if i in pairs:
                    i = pairs[i] + 1
                    continue
                if tokens[i].text == '<':
                    depth += 1
                elif tokens[i].text == '>':
                    depth -= 1
                i += 1
            if depth:
                raise RouteSyntaxError('unclosed generic argument list')
        elif i in pairs:
            i = pairs[i] + 1
        else:
            i += 1
    if start < hi:
        args.append((start, hi))
    return args


def _unwrap(tokens: list[Token], pairs: dict[int, int], lo: int, hi: int) -> tuple[int, int]:
    while lo < hi and tokens[lo].text == '(' and pairs.get(lo) == hi - 1:
        lo, hi = lo + 1, hi - 1
    return lo, hi


def decode_string(token: Token) -> str:
    if token.kind == 'raw_string':
        match = re.match(r'r(#{0,255})"', token.text)
        assert match
        return token.text[match.end():-(1 + len(match.group(1)))]
    if token.kind != 'string':
        raise RouteSyntaxError('route path is not a Rust str literal')
    text, out, i = token.text[1:-1], [], 0
    escapes = {'n': '\n', 'r': '\r', 't': '\t', '0': '\0', '\\': '\\', '"': '"', "'": "'"}
    while i < len(text):
        if text[i] != '\\':
            out.append(text[i])
            i += 1
            continue
        i += 1
        if i >= len(text):
            raise RouteSyntaxError('incomplete Rust string escape')
        if text[i] in escapes:
            out.append(escapes[text[i]])
            i += 1
        elif text[i] in '\r\n':
            while i < len(text) and text[i].isspace():
                i += 1
        elif text[i] == 'x' and re.fullmatch(r'[0-9A-Fa-f]{2}', text[i + 1:i + 3] or '-'):
            value = int(text[i + 1:i + 3], 16)
            if value > 127:
                raise RouteSyntaxError('non-ASCII Rust str hex escape')
            out.append(chr(value))
            i += 3
        elif text.startswith('u{', i):
            end = text.find('}', i + 2)
            digits = text[i + 2:end].replace('_', '') if end >= 0 else ''
            if not re.fullmatch(r'[0-9A-Fa-f]{1,6}', digits):
                raise RouteSyntaxError('invalid Unicode route escape')
            value = int(digits, 16)
            if value > 0x10ffff or 0xd800 <= value <= 0xdfff:
                raise RouteSyntaxError('invalid Unicode route scalar')
            out.append(chr(value))
            i = end + 1
        else:
            raise RouteSyntaxError('unsupported Rust string escape')
    return ''.join(out)


def _method_filter(tokens: list[Token], pairs: dict[int, int], lo: int, hi: int) -> set[str] | None:
    lo, hi = _unwrap(tokens, pairs, lo, hi)
    pieces = ''.join(t.text for t in tokens[lo:hi])
    # Only explicit bitwise unions are understood. Variables and calls retain
    # UNRESOLVED rather than borrowing methods from arbitrary expression bodies.
    parts = pieces.split('|')
    names = []
    for part in parts:
        match = re.fullmatch(r'(?:axum::routing::)?MethodFilter::([A-Z]+)', part)
        if not match or match.group(1) not in METHODS:
            return None
        names.append(match.group(1))
    return set(names)


def router_methods(tokens: list[Token], pairs: dict[int, int], lo: int, hi: int) -> tuple[set[str], bool]:
    lo, hi = _unwrap(tokens, pairs, lo, hi)
    if lo >= hi:
        return set(), False
    # Find the outer constructor path, stopping at its argument list. Never
    # scan handler expressions for get/post/etc.
    i = lo
    while i < hi and (tokens[i].kind == 'ident' or tokens[i].text == ':'):
        i += 1
    if i == lo or i >= hi or tokens[i].text != '(' or i not in pairs:
        return set(), False
    name = ''.join(token.text for token in tokens[lo:i])
    canonical = name.removeprefix('axum::routing::')
    method_name = canonical.removesuffix('_service').upper()
    found: set[str] = set()
    known = True
    if canonical in {'any', 'any_service'}:
        found.add('ANY')
    elif method_name in METHODS and '::' not in canonical:
        found.add(method_name)
    elif canonical in {'on', 'on_service'}:
        args = _arguments(tokens, pairs, i + 1, pairs[i])
        extracted = _method_filter(tokens, pairs, *args[0]) if args else None
        known = extracted is not None
        found.update(extracted or set())
    elif canonical != 'MethodRouter::new':
        known = False
    i = pairs[i] + 1
    while i < hi:
        if i + 2 >= hi or tokens[i].text != '.' or tokens[i + 1].kind != 'ident' or tokens[i + 2].text != '(':
            return found, False
        method, begin = tokens[i + 1].text, i + 2
        end = pairs[begin]
        if end >= hi:
            return found, False
        explicit = method.removesuffix('_service').upper()
        if explicit in METHODS:
            found.add(explicit)
        elif method in {'on', 'on_service'}:
            args = _arguments(tokens, pairs, begin + 1, end)
            extracted = _method_filter(tokens, pairs, *args[0]) if args else None
            known = known and extracted is not None
            found.update(extracted or set())
        elif method not in TRANSPARENT_METHODS:
            known = False
        i = end + 1
    return found, known and bool(found)


def extract_routes(source: str) -> list[RouteDeclaration]:
    tokens = tokenize(source)
    pairs = delimiter_pairs(tokens)
    declarations = []
    for index in range(len(tokens) - 2):
        if tokens[index].text != '.' or tokens[index].kind != 'punct':
            continue
        registration = tokens[index + 1].text.removeprefix('r#')
        if registration not in REGISTRATIONS or tokens[index + 1].kind != 'ident':
            continue
        begin = index + 2
        if tokens[begin].text != '(':
            # Explicit generic route registrations are not silently lost.
            if tokens[begin].text == ':':
                raise _error(source, tokens[index].offset, 'unsupported generic route registration')
            continue
        end = pairs[begin]
        args = _arguments(tokens, pairs, begin + 1, end)
        # A zero-argument application method named `route` cannot be an Axum
        # route registration, whose API requires both path and method-router
        # arguments. Keep malformed one/three-argument registrations fatal.
        if not args:
            continue
        if len(args) != 2 or any(lo >= hi for lo, hi in args):
            raise _error(source, tokens[index].offset, 'route registration must have two arguments')
        lo, hi = _unwrap(tokens, pairs, *args[0])
        value = decode_string(tokens[lo]) if hi == lo + 1 and tokens[lo].kind in {'string', 'raw_string'} else None
        observed, known = router_methods(tokens, pairs, *args[1]) if registration == 'route' else (set(), False)
        start_offset, end_offset = tokens[index].offset, tokens[end].end
        declarations.append(RouteDeclaration(
            value, registration, tuple(sorted(observed)) if known else ('UNRESOLVED',),
            tuple(sorted(observed)), 'literal' if value is not None else 'dynamic',
            'explicit_outer_router' if known else 'unresolved', start_offset, end_offset,
            'sha256:' + hashlib.sha256(source[start_offset:end_offset].encode()).hexdigest(),
        ))
    return declarations
