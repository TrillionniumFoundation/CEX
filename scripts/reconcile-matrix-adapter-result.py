#!/usr/bin/env python3
"""Reconcile one held Matrix adapter delivery from a causally bound durable lookup.

The command performs exactly one read-only adapter lookup and one least-privilege
PostgreSQL function call. It never resubmits the original business request or a
Matrix message. Remote PostgreSQL is accepted only with certificate-verified TLS.
Secrets are read from a closed environment and never placed in process arguments
or result JSON.
"""
from __future__ import annotations

import argparse
import base64
from datetime import datetime, timezone
import hashlib
import ipaddress
import json
import os
from pathlib import Path
import re
import stat
import subprocess
import sys
import tempfile
from typing import Any, Callable
from urllib.error import HTTPError, URLError
from urllib.parse import unquote, urlsplit, urlunsplit
from urllib.request import HTTPRedirectHandler, Request, build_opener
import uuid

SCHEMA = "cex.matrix.adapter-result-reconciler.v1"
SECURITY_CONTRACT = "v2"
LOOKUP_PATH = "/v1/matrix/results/lookup"
LOOKUP_SCHEMA = "cex.matrix.adapter-result-reconciliation.v2"
LOOKUP_ACTION = "task_result_reconciled"
DELIVERY_FINGERPRINT_DOMAIN = "cex.matrix.adapter-result-delivery.v1"
EVIDENCE_SCHEMA = "cex.matrix.adapter-result-reconciliation-evidence.v2"
MAX_RESPONSE_BYTES = 1_048_576
MAX_ERROR_BYTES = 16_384
MAX_CA_BYTES = 1_048_576
DEFAULT_TIMEOUT_SECONDS = 20
DEFAULT_PSQL_TIMEOUT_SECONDS = 60
SHA256_RE = re.compile(r"^sha256:[0-9a-f]{64}$")
COMMIT_RE = re.compile(r"^[0-9a-f]{40}$")
ENV_NAME_RE = re.compile(r"^[A-Z][A-Z0-9_]{0,127}$")


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
        or any(
            character.isspace() or ord(character) < 32 or ord(character) == 127
            for character in value
        )
    ):
        raise ReconciliationError("invalid_matrix_identity")
    return value


def delivery_request_fingerprint(
    delivery_id: str,
    payload_sha256: str,
    event_id: str,
    sender: str,
    room_id: str,
) -> str:
    digest = hashlib.sha256()
    for value in (
        DELIVERY_FINGERPRINT_DOMAIN,
        delivery_id,
        payload_sha256,
        event_id,
        sender,
        room_id,
    ):
        encoded = value.encode("utf-8")
        digest.update(len(encoded).to_bytes(8, "big"))
        digest.update(encoded)
    return "sha256:" + digest.hexdigest()


def validate_inputs(args: argparse.Namespace) -> None:
    try:
        parsed_delivery = uuid.UUID(args.delivery_id)
    except (ValueError, AttributeError):
        raise ReconciliationError("invalid_delivery_id") from None
    if str(parsed_delivery) != args.delivery_id:
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
    for name in (args.adapter_token_env, args.database_url_env):
        if not ENV_NAME_RE.fullmatch(name):
            raise ReconciliationError("invalid_secret_environment_name")


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
            "user-agent": "cex-matrix-result-reconciler/2",
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
        or value.get("delivery_id") != expected["delivery_id"]
        or value.get("event_id") != expected["event_id"]
        or value.get("room_id") != expected["room_id"]
        or value.get("sender") != expected["sender"]
        or value.get("payload_sha256") != expected["payload_sha256"]
        or value.get("request_fingerprint") != expected["request_fingerprint"]
        or value.get("projected_reply") is not None
        or not isinstance(reconciliation, dict)
        or reconciliation.get("schema") != LOOKUP_SCHEMA
        or reconciliation.get("source") != "consumer_entry_durable_replay"
        or reconciliation.get("read_only") is not True
        or reconciliation.get("causal_binding") != "delivery_payload_fingerprint"
        or not isinstance(forwarded, dict)
    ):
        raise ReconciliationError("lookup_response_identity_mismatch")

    recomputed = delivery_request_fingerprint(
        expected["delivery_id"],
        expected["payload_sha256"],
        expected["event_id"],
        expected["sender"],
        expected["room_id"],
    )
    if recomputed != expected["request_fingerprint"]:
        raise ReconciliationError("lookup_response_request_fingerprint_mismatch")

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


def is_loopback_host(host: str) -> bool:
    try:
        return ipaddress.ip_address(host).is_loopback
    except ValueError:
        return False


def trusted_regular_path(
    raw: str,
    *,
    error_code: str,
    max_bytes: int | None = None,
    executable: bool = False,
) -> str:
    path = Path(raw)
    if not path.is_absolute():
        raise ReconciliationError(error_code)
    try:
        resolved = path.resolve(strict=True)
        metadata = path.lstat()
        parent = path.parent.stat()
    except OSError:
        raise ReconciliationError(error_code) from None
    trusted_owners = {0, os.geteuid()}
    if (
        resolved != path
        or not stat.S_ISREG(metadata.st_mode)
        or metadata.st_nlink != 1
        or metadata.st_uid not in trusted_owners
        or metadata.st_mode & 0o022
        or not stat.S_ISDIR(parent.st_mode)
        or parent.st_uid not in trusted_owners
        or parent.st_mode & 0o022
        or (max_bytes is not None and not 1 <= metadata.st_size <= max_bytes)
        or (executable and not os.access(path, os.X_OK))
    ):
        raise ReconciliationError(error_code)
    return str(path)


def parse_database_url(
    raw: str,
    ca_file: str,
    allow_insecure_loopback: bool,
) -> dict[str, str]:
    parsed = urlsplit(raw)
    if (
        parsed.scheme not in {"postgres", "postgresql"}
        or not parsed.hostname
        or parsed.username is None
        or parsed.password is None
        or parsed.query
        or parsed.fragment
        or not parsed.path.startswith("/")
    ):
        raise ReconciliationError("invalid_database_url")
    database = unquote(parsed.path[1:])
    username = unquote(parsed.username)
    password = unquote(parsed.password)
    host = parsed.hostname
    if (
        not database
        or "/" in database
        or not username
        or not password
        or len(username.encode("utf-8")) > 63
        or len(password.encode("utf-8")) > 4096
        or len(database.encode("utf-8")) > 63
        or len(host.encode("utf-8")) > 253
        or any(character in host for character in ",/%\\")
        or any(
            character.isspace() or ord(character) < 32 or ord(character) == 127
            for character in database + username + password + host
        )
    ):
        raise ReconciliationError("invalid_database_identity")
    try:
        port = parsed.port or 5432
    except ValueError:
        raise ReconciliationError("invalid_database_port") from None
    if not 1 <= port <= 65535:
        raise ReconciliationError("invalid_database_port")

    database_environment = {
        "PGHOST": host,
        "PGPORT": str(port),
        "PGDATABASE": database,
        "PGUSER": username,
        "PGPASSWORD": password,
        "PGCONNECT_TIMEOUT": "10",
        "PGTARGETSESSIONATTRS": "read-write",
    }
    if allow_insecure_loopback and is_loopback_host(host):
        database_environment["PGSSLMODE"] = "disable"
    else:
        database_environment["PGSSLMODE"] = "verify-full"
        database_environment["PGSSLROOTCERT"] = trusted_regular_path(
            ca_file,
            error_code="database_ca_required_or_unsafe",
            max_bytes=MAX_CA_BYTES,
        )
        database_environment["PGCHANNELBINDING"] = "require"
    return database_environment


def resolve_psql_path(raw: str) -> str:
    if not raw:
        raise ReconciliationError("pinned_psql_path_required")
    return trusted_regular_path(
        raw,
        error_code="pinned_psql_unavailable_or_unsafe",
        executable=True,
    )


def b64_text(value: str) -> str:
    return base64.b64encode(value.encode("utf-8")).decode("ascii")


def sql_text(value: str) -> str:
    return f"convert_from(decode('{b64_text(value)}','base64'),'UTF8')"


def build_sql(
    args: argparse.Namespace,
    forwarded: dict[str, Any],
    lookup_sha256: str,
    observed_at: str,
    request_fingerprint: str,
) -> str:
    forwarded_text = json.dumps(
        forwarded,
        ensure_ascii=False,
        separators=(",", ":"),
        sort_keys=True,
    )
    evidence = {
        "schema": EVIDENCE_SCHEMA,
        "lookup_response_sha256": lookup_sha256,
        "observed_at": observed_at,
        "candidate_sha": args.candidate_sha,
        "request_fingerprint": request_fingerprint,
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
    # cex_matrix_reconcile_adapter_result_v1 is intentionally not called by v2.
    return f"""set client_min_messages = warning;
set search_path = pg_catalog, public;
select 'CEX_MATRIX_RECONCILIATION:' || public.cex_matrix_reconcile_adapter_result_v2(
    ({sql_text(args.delivery_id)})::uuid,
    {sql_text(args.event_id)},
    {sql_text(args.payload_sha256)},
    {sql_text(args.room_id)},
    {sql_text(args.sender)},
    {sql_text(task_id)},
    {sql_text(request_fingerprint)},
    {result_expr},
    'sha256:' || encode(sha256(convert_to(({result_expr})::text, 'UTF8')), 'hex'),
    {evidence_expr}
);
"""


def psql_environment(database: dict[str, str]) -> dict[str, str]:
    # Closed construction replaces the old environment.pop("DATABASE_URL", None) pattern.
    environment = {
        "LANG": "C.UTF-8",
        "LC_ALL": "C.UTF-8",
        "HOME": "/nonexistent",
        "PGAPPNAME": "cex-matrix-result-reconciler-v2",
        "PGPASSFILE": os.devnull,
        "PSQLRC": os.devnull,
        "PGOPTIONS": (
            "-c statement_timeout=60000 "
            "-c lock_timeout=5000 "
            "-c idle_in_transaction_session_timeout=60000"
        ),
    }
    environment.update(database)
    return environment


def run_psql(
    database: dict[str, str],
    sql: str,
    timeout_seconds: int,
    psql_path: str,
    runner: Callable[..., subprocess.CompletedProcess[str]] = subprocess.run,
) -> str:
    executable = resolve_psql_path(psql_path)
    environment = psql_environment(database)
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
        digest = hashlib.sha256(
            completed.stderr.encode("utf-8", "replace")
        ).hexdigest()
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
        "delivery_id": "61000000-0000-4000-8000-000000000001",
        "event_id": "$event",
        "room_id": "!room:example",
        "sender": "@alice:example",
        "payload_sha256": "sha256:" + "2" * 64,
    }
    expected["request_fingerprint"] = delivery_request_fingerprint(
        expected["delivery_id"],
        expected["payload_sha256"],
        expected["event_id"],
        expected["sender"],
        expected["room_id"],
    )
    forwarded = {
        "task_id": "task-1",
        "source": {
            "kind": "matrix_message",
            "event_id": expected["event_id"],
            "room_id": expected["room_id"],
            "matrix_user_id": expected["sender"],
            "identity_scope": {
                "user_id": expected["sender"],
                "room_id": expected["room_id"],
            },
        },
        "raw": {"invocation_id": "task-1"},
    }

    def envelope(generated_at: str) -> bytes:
        value = {
            "accepted": True,
            "action": LOOKUP_ACTION,
            **expected,
            "forwarded": forwarded,
            "projected_reply": None,
            "reconciliation": {
                "schema": LOOKUP_SCHEMA,
                "source": "consumer_entry_durable_replay",
                "read_only": True,
                "causal_binding": "delivery_payload_fingerprint",
            },
            "generated_at": generated_at,
        }
        return json.dumps(value, separators=(",", ":"), sort_keys=True).encode()

    first_raw = envelope("2026-09-09T00:00:00Z")
    retry_raw = envelope("2026-09-09T00:00:01Z")
    assert parse_lookup_response(first_raw, expected) == forwarded
    assert parse_lookup_response(retry_raw, expected) == forwarded
    assert hashlib.sha256(first_raw).digest() != hashlib.sha256(retry_raw).digest()

    changed = json.loads(first_raw)
    changed["payload_sha256"] = "sha256:" + "9" * 64
    try:
        parse_lookup_response(json.dumps(changed).encode(), expected)
    except ReconciliationError:
        pass
    else:
        raise AssertionError("delivery payload mismatch accepted")
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

    with tempfile.TemporaryDirectory(prefix="cex-reconciler-self-test-") as directory:
        root = Path(directory)
        ca = root / "root-ca.pem"
        ca.write_text("test-only-ca\n", encoding="utf-8")
        ca.chmod(0o600)
        psql = root / "psql"
        psql.write_text("#!/bin/sh\nexit 0\n", encoding="utf-8")
        psql.chmod(0o700)

        remote = parse_database_url(
            "postgresql://user:pass@db.example:5433/cex",
            str(ca),
            False,
        )
        assert remote["PGSSLMODE"] == "verify-full"
        assert remote["PGSSLROOTCERT"] == str(ca)
        assert remote["PGCHANNELBINDING"] == "require"
        assert resolve_psql_path(str(psql)) == str(psql)

        closed = psql_environment(remote)
        for hostile in (
            "PGSERVICE",
            "PGSERVICEFILE",
            "PGSYSCONFDIR",
            "PGREQUIRESSL",
            "PGHOSTADDR",
            "DATABASE_URL",
            "MATRIX_RECONCILIATION_DATABASE_URL",
        ):
            assert hostile not in closed

        local = parse_database_url(
            "postgresql://user:pass@127.0.0.1:5432/cex",
            "",
            True,
        )
        assert local["PGSSLMODE"] == "disable"
        try:
            parse_database_url(
                "postgresql://user:pass@localhost:5432/cex",
                "",
                True,
            )
        except ReconciliationError:
            pass
        else:
            raise AssertionError("non-literal loopback database accepted")
        try:
            parse_database_url(
                "postgresql://user:pass@db.example:5432/cex",
                "",
                False,
            )
        except ReconciliationError:
            pass
        else:
            raise AssertionError("remote database without CA accepted")
        try:
            parse_database_url(
                "postgresql://user:@db.example:5432/cex",
                str(ca),
                False,
            )
        except ReconciliationError:
            pass
        else:
            raise AssertionError("empty database password accepted")

    invalid_args = argparse.Namespace(
        delivery_id="61000000-0000-4000-8000-00000000000A",
        event_id=expected["event_id"],
        room_id=expected["room_id"],
        sender=expected["sender"],
        payload_sha256=expected["payload_sha256"],
        candidate_sha="a" * 40,
        timeout_seconds=DEFAULT_TIMEOUT_SECONDS,
        psql_timeout_seconds=DEFAULT_PSQL_TIMEOUT_SECONDS,
        adapter_token_env="MATRIX_ENTRY_INGRESS_TOKEN",
        database_url_env="MATRIX_RECONCILIATION_DATABASE_URL",
    )
    try:
        validate_inputs(invalid_args)
    except ReconciliationError:
        pass
    else:
        raise AssertionError("noncanonical uppercase delivery UUID accepted")

    args = argparse.Namespace(
        delivery_id=expected["delivery_id"],
        event_id=expected["event_id"],
        room_id=expected["room_id"],
        sender=expected["sender"],
        payload_sha256=expected["payload_sha256"],
        candidate_sha="a" * 40,
    )
    sql = build_sql(
        args,
        forwarded,
        "sha256:" + "3" * 64,
        "2026-09-09T00:00:00+00:00",
        expected["request_fingerprint"],
    )
    assert "cex_matrix_reconcile_adapter_result_v2" in sql
    assert expected["request_fingerprint"] not in sql
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
    result.add_argument(
        "--database-ca-file",
        default=os.environ.get("MATRIX_RECONCILIATION_DATABASE_CA_FILE", ""),
    )
    result.add_argument(
        "--psql-path",
        default=os.environ.get("MATRIX_RECONCILIATION_PSQL", ""),
    )
    result.add_argument("--timeout-seconds", type=int, default=DEFAULT_TIMEOUT_SECONDS)
    result.add_argument(
        "--psql-timeout-seconds",
        type=int,
        default=DEFAULT_PSQL_TIMEOUT_SECONDS,
    )
    result.add_argument("--allow-http-loopback", action="store_true")
    result.add_argument(
        "--allow-insecure-database-loopback",
        action="store_true",
        help="Test-only: permit sslmode=disable only for a loopback PostgreSQL host.",
    )
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
                    "security_contract": SECURITY_CONTRACT,
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
        database = parse_database_url(
            database_url,
            args.database_ca_file,
            args.allow_insecure_database_loopback,
        )
        psql_path = resolve_psql_path(args.psql_path)
        request_fingerprint = delivery_request_fingerprint(
            args.delivery_id,
            args.payload_sha256,
            args.event_id,
            args.sender,
            args.room_id,
        )
        expected = {
            "delivery_id": args.delivery_id,
            "event_id": args.event_id,
            "room_id": args.room_id,
            "sender": args.sender,
            "payload_sha256": args.payload_sha256,
            "request_fingerprint": request_fingerprint,
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
        sql = build_sql(
            args,
            forwarded,
            lookup_sha256,
            observed_at,
            request_fingerprint,
        )
        disposition = run_psql(
            database,
            sql,
            args.psql_timeout_seconds,
            psql_path,
        )
        print(
            json.dumps(
                {
                    "schema": SCHEMA,
                    "status": "ok",
                    "security_contract": SECURITY_CONTRACT,
                    "delivery_id": args.delivery_id,
                    "event_id": args.event_id,
                    "room_id": args.room_id,
                    "sender": args.sender,
                    "payload_sha256": args.payload_sha256,
                    "request_fingerprint": request_fingerprint,
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
