#!/usr/bin/env python3
"""Reconcile one held Matrix adapter delivery from a principal-bound durable lookup.

The command performs exactly one read-only adapter lookup and one least-privilege
PostgreSQL function call. It never resubmits the original business request or a
Matrix message. Secrets are read from environment variables and never placed in
process arguments or result JSON.
"""
from __future__ import annotations

import argparse
import base64
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
from typing import Any, Callable
from urllib.error import HTTPError, URLError
from urllib.parse import unquote, urlsplit, urlunsplit
from urllib.request import HTTPRedirectHandler, Request, build_opener
import uuid

SCHEMA = "cex.matrix.adapter-result-reconciler.v1"
LOOKUP_PATH = "/v1/matrix/results/lookup"
LOOKUP_SCHEMA = "cex.matrix.adapter-result-reconciliation.v1"
LOOKUP_ACTION = "task_result_reconciled"
MAX_RESPONSE_BYTES = 1_048_576
MAX_ERROR_BYTES = 16_384
DEFAULT_TIMEOUT_SECONDS = 20
DEFAULT_PSQL_TIMEOUT_SECONDS = 60
SHA256_RE = re.compile(r"^sha256:[0-9a-f]{64}$")
COMMIT_RE = re.compile(r"^[0-9a-f]{40}$")


class ReconciliationError(RuntimeError):
    """A bounded failure safe to expose only through its stable code."""


class NoRedirect(HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):  # noqa: ANN001
        return None


def unique_object(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    result: dict[str, Any] = {}
    for key, value in pairs:
        if key in result:
            raise ReconciliationError("duplicate_json_field")
        result[key] = value
    return result


def validate_matrix_identifier(value: str, prefix: str) -> str:
    if (
        not isinstance(value, str)
        or len(value.encode("utf-8")) not in range(2, 513)
        or not value.startswith(prefix)
        or any(character.isspace() or ord(character) < 32 or ord(character) == 127 for character in value)
    ):
        raise ReconciliationError("invalid_matrix_identity")
    return value


def validate_inputs(args: argparse.Namespace) -> None:
    try:
        parsed_delivery = uuid.UUID(args.delivery_id)
    except (ValueError, AttributeError):
        raise ReconciliationError("invalid_delivery_id") from None
    if str(parsed_delivery) != args.delivery_id.lower():
        raise ReconciliationError("noncanonical_delivery_id")
    validate_matrix_identifier(args.event_id, "$")
    validate_matrix_identifier(args.room_id, "!")
    validate_matrix_identifier(args.sender, "@")
    if not SHA256_RE.fullmatch(args.payload_sha256):
        raise ReconciliationError("invalid_payload_sha256")
    if not COMMIT_RE.fullmatch(args.candidate_sha):
        raise ReconciliationError("invalid_candidate_sha")
    if not 1 <= args.timeout_seconds <= 120:
        raise ReconciliationError("invalid_http_timeout")
    if not 1 <= args.psql_timeout_seconds <= 300:
        raise ReconciliationError("invalid_psql_timeout")


def lookup_url(base: str, allow_http_loopback: bool) -> str:
    parsed = urlsplit(base)
    if (
        parsed.scheme not in {"http", "https"}
        or not parsed.hostname
        or parsed.username is not None
        or parsed.password is not None
        or parsed.query
        or parsed.fragment
    ):
        raise ReconciliationError("invalid_adapter_url")
    if parsed.scheme != "https":
        loopback = parsed.hostname.lower() in {"127.0.0.1", "::1", "localhost"}
        if not allow_http_loopback or not loopback:
            raise ReconciliationError("adapter_https_required")
    path = parsed.path.rstrip("/") + LOOKUP_PATH
    return urlunsplit((parsed.scheme, parsed.netloc, path, "", ""))


def read_bounded(stream, limit: int) -> bytes:  # noqa: ANN001
    declared = stream.headers.get("Content-Length")
    if declared is not None:
        try:
            if int(declared) > limit:
                raise ReconciliationError("lookup_response_too_large")
        except ValueError:
            raise ReconciliationError("invalid_content_length") from None
    body = stream.read(limit + 1)
    if len(body) > limit:
        raise ReconciliationError("lookup_response_too_large")
    return body


def fetch_lookup(
    url: str,
    token: str,
    request_body: dict[str, str],
    timeout_seconds: int,
    opener=None,  # noqa: ANN001
) -> bytes:
    if len(token.encode("utf-8")) < 32:
        raise ReconciliationError("adapter_token_missing_or_short")
    payload = json.dumps(
        request_body,
        ensure_ascii=False,
        separators=(",", ":"),
        sort_keys=True,
    ).encode("utf-8")
    request = Request(
        url,
        data=payload,
        method="POST",
        headers={
            "content-type": "application/json",
            "accept": "application/json",
            "x-entry-token": token,
            "user-agent": "cex-matrix-result-reconciler/1",
        },
    )
    client = opener or build_opener(NoRedirect())
    try:
        with client.open(request, timeout=timeout_seconds) as response:
            status = getattr(response, "status", response.getcode())
            body = read_bounded(response, MAX_RESPONSE_BYTES)
    except HTTPError as error:
        try:
            read_bounded(error, MAX_ERROR_BYTES)
        except Exception:
            pass
        raise ReconciliationError("adapter_lookup_rejected") from None
    except (URLError, TimeoutError, OSError):
        raise ReconciliationError("adapter_lookup_unavailable") from None
    if status != 200:
        raise ReconciliationError("adapter_lookup_non_success")
    return body


def parse_lookup_response(raw: bytes, expected: dict[str, str]) -> dict[str, Any]:
    try:
        value = json.loads(raw, object_pairs_hook=unique_object)
    except (UnicodeDecodeError, json.JSONDecodeError):
        raise ReconciliationError("lookup_response_invalid_json") from None
    if not isinstance(value, dict):
        raise ReconciliationError("lookup_response_invalid_shape")
    reconciliation = value.get("reconciliation")
    forwarded = value.get("forwarded")
    if (
        value.get("accepted") is not True
        or value.get("action") != LOOKUP_ACTION
        or value.get("event_id") != expected["event_id"]
        or value.get("room_id") != expected["room_id"]
        or value.get("sender") != expected["sender"]
        or value.get("projected_reply") is not None
        or not isinstance(reconciliation, dict)
        or reconciliation.get("schema") != LOOKUP_SCHEMA
        or reconciliation.get("source") != "consumer_entry_durable_replay"
        or reconciliation.get("read_only") is not True
        or not isinstance(forwarded, dict)
    ):
        raise ReconciliationError("lookup_response_identity_mismatch")

    source = forwarded.get("source")
    task_id = forwarded.get("task_id")
    if (
        not isinstance(source, dict)
        or source.get("kind") != "matrix_message"
        or source.get("event_id") != expected["event_id"]
        or source.get("room_id") != expected["room_id"]
        or source.get("matrix_user_id") != expected["sender"]
        or not isinstance(task_id, str)
        or not 1 <= len(task_id.encode("utf-8")) <= 128
    ):
        raise ReconciliationError("lookup_forwarded_identity_mismatch")
    identity_scope = source.get("identity_scope")
    if (
        not isinstance(identity_scope, dict)
        or identity_scope.get("user_id") != expected["sender"]
        or identity_scope.get("room_id") != expected["room_id"]
    ):
        raise ReconciliationError("lookup_forwarded_scope_mismatch")
    raw_result = forwarded.get("raw")
    if raw_result is not None:
        if not isinstance(raw_result, dict):
            raise ReconciliationError("lookup_forwarded_raw_invalid")
        invocation_id = raw_result.get("invocation_id")
        if invocation_id is not None and invocation_id != task_id:
            raise ReconciliationError("lookup_forwarded_task_mismatch")
    return forwarded


def parse_database_url(raw: str) -> dict[str, str]:
    parsed = urlsplit(raw)
    if (
        parsed.scheme not in {"postgres", "postgresql"}
        or not parsed.hostname
        or parsed.username is None
        or parsed.query
        or parsed.fragment
        or not parsed.path.startswith("/")
    ):
        raise ReconciliationError("invalid_database_url")
    database = unquote(parsed.path[1:])
    username = unquote(parsed.username)
    password = unquote(parsed.password or "")
    if (
        not database
        or "/" in database
        or any(character.isspace() or ord(character) < 32 for character in database + username)
    ):
        raise ReconciliationError("invalid_database_identity")
    try:
        port = parsed.port or 5432
    except ValueError:
        raise ReconciliationError("invalid_database_port") from None
    if not 1 <= port <= 65535:
        raise ReconciliationError("invalid_database_port")
    return {
        "PGHOST": parsed.hostname,
        "PGPORT": str(port),
        "PGDATABASE": database,
        "PGUSER": username,
        "PGPASSWORD": password,
    }


def b64_text(value: str) -> str:
    return base64.b64encode(value.encode("utf-8")).decode("ascii")


def sql_text(value: str) -> str:
    return f"convert_from(decode('{b64_text(value)}','base64'),'UTF8')"


def build_sql(
    args: argparse.Namespace,
    forwarded: dict[str, Any],
    lookup_sha256: str,
    observed_at: str,
) -> str:
    forwarded_text = json.dumps(
        forwarded,
        ensure_ascii=False,
        separators=(",", ":"),
        sort_keys=True,
    )
    evidence = {
        "schema": "cex.matrix.adapter-result-reconciliation-evidence.v1",
        "lookup_response_sha256": lookup_sha256,
        "observed_at": observed_at,
        "candidate_sha": args.candidate_sha,
    }
    evidence_text = json.dumps(
        evidence,
        ensure_ascii=False,
        separators=(",", ":"),
        sort_keys=True,
    )
    result_expr = f"({sql_text(forwarded_text)})::jsonb"
    evidence_expr = f"({sql_text(evidence_text)})::jsonb"
    task_id = forwarded["task_id"]
    return f"""set client_min_messages = warning;
set search_path = pg_catalog, public;
select 'CEX_MATRIX_RECONCILIATION:' || public.cex_matrix_reconcile_adapter_result_v1(
    ({sql_text(args.delivery_id)})::uuid,
    {sql_text(args.event_id)},
    {sql_text(args.payload_sha256)},
    {sql_text(args.room_id)},
    {sql_text(args.sender)},
    {sql_text(task_id)},
    {result_expr},
    'sha256:' || encode(sha256(convert_to(({result_expr})::text, 'UTF8')), 'hex'),
    {evidence_expr}
);
"""


def run_psql(
    database: dict[str, str],
    sql: str,
    timeout_seconds: int,
    runner: Callable[..., subprocess.CompletedProcess[str]] = subprocess.run,
) -> str:
    explicit = os.environ.get("MATRIX_RECONCILIATION_PSQL")
    executable = explicit or shutil.which("psql")
    if not executable or (explicit and not Path(explicit).is_file()):
        raise ReconciliationError("psql_unavailable")
    environment = os.environ.copy()
    environment.pop("DATABASE_URL", None)
    environment.pop("MATRIX_RECONCILIATION_DATABASE_URL", None)
    environment.update(database)
    command = [
        executable,
        "-X",
        "--no-password",
        "-q",
        "-A",
        "-t",
        "-v",
        "ON_ERROR_STOP=1",
        "-f",
        "-",
    ]
    try:
        completed = runner(
            command,
            input=sql,
            text=True,
            capture_output=True,
            timeout=timeout_seconds,
            env=environment,
            check=False,
        )
    except (OSError, subprocess.TimeoutExpired):
        raise ReconciliationError("psql_execution_failed") from None
    if completed.returncode != 0:
        digest = hashlib.sha256(completed.stderr.encode("utf-8", "replace")).hexdigest()
        raise ReconciliationError("psql_reconciliation_rejected:" + digest)
    markers = [
        line.strip()
        for line in completed.stdout.splitlines()
        if line.strip().startswith("CEX_MATRIX_RECONCILIATION:")
    ]
    if len(markers) != 1:
        raise ReconciliationError("psql_result_marker_invalid")
    disposition = markers[0].split(":", 1)[1]
    if disposition not in {"reconciled", "replay"}:
        raise ReconciliationError("psql_disposition_invalid")
    return disposition


def self_test() -> None:
    expected = {
        "event_id": "$event",
        "room_id": "!room:example",
        "sender": "@alice:example",
    }
    forwarded = {
        "task_id": "task-1",
        "source": {
            "kind": "matrix_message",
            **expected,
            "matrix_user_id": expected["sender"],
            "identity_scope": {
                "user_id": expected["sender"],
                "room_id": expected["room_id"],
            },
        },
        "raw": {"invocation_id": "task-1"},
    }
    envelope = {
        "accepted": True,
        "action": LOOKUP_ACTION,
        **expected,
        "sender": expected["sender"],
        "forwarded": forwarded,
        "projected_reply": None,
        "reconciliation": {
            "schema": LOOKUP_SCHEMA,
            "source": "consumer_entry_durable_replay",
            "read_only": True,
        },
    }
    raw = json.dumps(envelope, separators=(",", ":"), sort_keys=True).encode()
    assert parse_lookup_response(raw, expected) == forwarded
    changed = dict(envelope)
    changed["sender"] = "@mallory:example"
    try:
        parse_lookup_response(json.dumps(changed).encode(), expected)
    except ReconciliationError:
        pass
    else:
        raise AssertionError("identity mismatch accepted")
    try:
        parse_lookup_response(b'{"accepted":true,"accepted":true}', expected)
    except ReconciliationError:
        pass
    else:
        raise AssertionError("duplicate JSON field accepted")
    assert lookup_url("https://adapter.example/proxy/", False) == (
        "https://adapter.example/proxy/v1/matrix/results/lookup"
    )
    try:
        lookup_url("http://adapter.example", True)
    except ReconciliationError:
        pass
    else:
        raise AssertionError("non-loopback HTTP accepted")
    database = parse_database_url("postgresql://user:pass@db.example:5433/cex")
    assert database["PGHOST"] == "db.example" and database["PGDATABASE"] == "cex"
    args = argparse.Namespace(
        delivery_id="61000000-0000-4000-8000-000000000001",
        event_id=expected["event_id"],
        room_id=expected["room_id"],
        sender=expected["sender"],
        payload_sha256="sha256:" + "2" * 64,
        candidate_sha="a" * 40,
    )
    sql = build_sql(args, forwarded, "sha256:" + "3" * 64, "2026-09-09T00:00:00+00:00")
    assert "cex_matrix_reconcile_adapter_result_v1" in sql
    assert "task-1" not in sql and "@alice:example" not in sql


def parser() -> argparse.ArgumentParser:
    result = argparse.ArgumentParser(description=__doc__)
    result.add_argument("--delivery-id")
    result.add_argument("--event-id")
    result.add_argument("--room-id")
    result.add_argument("--sender")
    result.add_argument("--payload-sha256")
    result.add_argument("--candidate-sha")
    result.add_argument(
        "--adapter-base-url",
        default=os.environ.get("MATRIX_ENTRY_ADAPTER_BASE_URL", ""),
    )
    result.add_argument(
        "--adapter-token-env",
        default="MATRIX_ENTRY_INGRESS_TOKEN",
    )
    result.add_argument(
        "--database-url-env",
        default="MATRIX_RECONCILIATION_DATABASE_URL",
    )
    result.add_argument("--timeout-seconds", type=int, default=DEFAULT_TIMEOUT_SECONDS)
    result.add_argument(
        "--psql-timeout-seconds",
        type=int,
        default=DEFAULT_PSQL_TIMEOUT_SECONDS,
    )
    result.add_argument("--allow-http-loopback", action="store_true")
    result.add_argument("--self-test", action="store_true")
    return result


def main() -> int:
    args = parser().parse_args()
    if args.self_test:
        try:
            self_test()
        except Exception:
            print("matrix_adapter_result_reconciler_self_test_failed", file=sys.stderr)
            return 1
        print(
            json.dumps(
                {
                    "schema": SCHEMA,
                    "status": "ok",
                    "self_test": True,
                    "production_authorization": "not_granted",
                },
                sort_keys=True,
            )
        )
        return 0

    try:
        required = (
            "delivery_id",
            "event_id",
            "room_id",
            "sender",
            "payload_sha256",
            "candidate_sha",
        )
        if any(not getattr(args, field) for field in required):
            raise ReconciliationError("required_argument_missing")
        validate_inputs(args)
        url = lookup_url(args.adapter_base_url, args.allow_http_loopback)
        token = os.environ.get(args.adapter_token_env, "")
        database_url = os.environ.get(args.database_url_env, "")
        if not database_url:
            raise ReconciliationError("database_url_missing")
        expected = {
            "event_id": args.event_id,
            "room_id": args.room_id,
            "sender": args.sender,
        }
        raw = fetch_lookup(
            url,
            token,
            expected,
            args.timeout_seconds,
        )
        forwarded = parse_lookup_response(raw, expected)
        lookup_sha256 = "sha256:" + hashlib.sha256(raw).hexdigest()
        observed_at = datetime.now(timezone.utc).isoformat()
        sql = build_sql(args, forwarded, lookup_sha256, observed_at)
        disposition = run_psql(
            parse_database_url(database_url),
            sql,
            args.psql_timeout_seconds,
        )
        print(
            json.dumps(
                {
                    "schema": SCHEMA,
                    "status": "ok",
                    "delivery_id": args.delivery_id,
                    "event_id": args.event_id,
                    "room_id": args.room_id,
                    "sender": args.sender,
                    "candidate_sha": args.candidate_sha,
                    "lookup_response_sha256": lookup_sha256,
                    "disposition": disposition,
                    "production_authorization": "not_granted",
                },
                sort_keys=True,
            )
        )
        return 0
    except ReconciliationError as error:
        code = str(error).split(":", 1)[0]
        print(code, file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
