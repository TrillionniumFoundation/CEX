#!/usr/bin/env python3
"""Exercise the real v3 CLI process against disposable PostgreSQL and a loopback HTTP fixture.

The HTTP response is synthetic, not a real Adapter/Consumer/homeserver. The CLI,
libpq connection, SQL functions and role enforcement are real in database mode.
No production targets, migrations, table cleanup or release decisions are performed.
"""
from __future__ import annotations

import argparse
import base64
import copy
from datetime import datetime, timezone
import hashlib
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import os
from pathlib import Path
import re
import secrets
import signal
import stat
import subprocess
import sys
import tempfile
import threading
import unittest
from urllib.parse import unquote, urlsplit
import uuid

ROOT = Path(__file__).resolve().parents[1]
CLI = 'scripts/reconcile-matrix-adapter-result.py'
SOURCES = (CLI, 'scripts/reconcile-matrix-adapter-result-v3.py',
           'scripts/reconcile-matrix-adapter-result-v2-core.py',
           'scripts/reconcile-matrix-adapter-result-v2-internal.py',
           'scripts/test-matrix-cli-postgres.py')
DOMAIN = 'cex.matrix.adapter-result-delivery.v1'
FUNCTIONS = ('cex_matrix_reconcile_adapter_result_v1',
             'cex_matrix_reconcile_adapter_result_v2',
             'cex_matrix_reconcile_adapter_result_v3')
LIMIT = 1024 * 1024


class Failure(RuntimeError):
    """A stable diagnostic; never include subprocess output or secrets."""


def need(condition: bool, code: str) -> None:
    if not condition:
        raise Failure(code)


def wire(value: object) -> bytes:
    return json.dumps(value, ensure_ascii=False, sort_keys=True,
                      separators=(',', ':'), allow_nan=False).encode('utf-8')


def digest(data: bytes) -> str:
    return 'sha256:' + hashlib.sha256(data).hexdigest()


def fingerprint(values: dict[str, str]) -> str:
    frame = bytearray()
    for value in (DOMAIN, *(values[k] for k in
                 ('delivery_id', 'payload_sha256', 'event_id', 'sender', 'room_id'))):
        encoded = value.encode('utf-8')
        frame.extend(len(encoded).to_bytes(8, 'big'))
        frame.extend(encoded)
    return digest(bytes(frame))


def text_sql(value: str) -> str:
    encoded = base64.b64encode(value.encode('utf-8')).decode('ascii')
    return f"convert_from(decode('{encoded}','base64'),'UTF8')"


def config(env: dict[str, str]) -> dict[str, str]:
    need(env.get('MATRIX_TEST_ALLOW_SCHEMA_RESET') == '1'
         and env.get('MATRIX_CLI_TEST_ALLOW_ROLE_CREATE') == '1', 'explicit_disposable_consent_required')
    try:
        u = urlsplit(env.get('MATRIX_TEST_DATABASE_URL', ''))
        port = u.port or 5432
        user, password = unquote(u.username or ''), unquote(u.password or '')
        need(u.scheme in ('postgres', 'postgresql') and u.hostname == '127.0.0.1'
             and u.path == '/matrix_review_ci' and not u.query and not u.fragment
             and 1 <= port <= 65535 and bool(user) and bool(password), 'disposable_target_required')
        need(re.fullmatch(r'[A-Za-z_][A-Za-z0-9_]{0,62}', user) is not None
             and len(password) <= 4096
             and not any(c.isspace() or ord(c) < 32 for c in password), 'invalid_database_identity')
    except (ValueError, UnicodeError):
        raise Failure('invalid_database_configuration') from None
    return {'PGHOST': '127.0.0.1', 'PGPORT': str(port), 'PGDATABASE': 'matrix_review_ci',
            'PGUSER': user, 'PGPASSWORD': password, 'PGSSLMODE': 'disable',
            'PGCONNECT_TIMEOUT': '5', 'PGPASSFILE': os.devnull, 'PSQLRC': os.devnull,
            'PGOPTIONS': '-c statement_timeout=10000 -c lock_timeout=2000',
            'LANG': 'C.UTF-8', 'LC_ALL': 'C.UTF-8', 'HOME': '/nonexistent'}


def bounded(args: list[str], env: dict[str, str], data: str = '', timeout: int = 30) -> tuple[int, str, str]:
    with tempfile.TemporaryFile() as out, tempfile.TemporaryFile() as err:
        process = subprocess.Popen(args, stdin=subprocess.PIPE, stdout=out, stderr=err,
                                   env=env, cwd=ROOT, start_new_session=os.name == 'posix')
        try:
            process.communicate(data.encode('utf-8'), timeout=timeout)
        except subprocess.TimeoutExpired:
            if os.name == 'posix':
                os.killpg(process.pid, signal.SIGKILL)
            else:
                process.kill()
            process.communicate()
            raise Failure('subprocess_timeout') from None
        need(out.tell() + err.tell() <= LIMIT, 'subprocess_output_limit')
        out.seek(0); err.seek(0)
        return process.returncode, out.read().decode('utf-8'), err.read().decode('utf-8')


def client_path() -> str:
    # Debian's /usr/bin/psql may be a pg_wrapper symlink. Select a real client,
    # not a PATH-injected wrapper; the canonical CLI revalidates its custody.
    candidates = sorted(Path('/usr/lib/postgresql').glob('*/bin/psql'), reverse=True)
    for p in candidates:
        m = p.lstat()
        if (p.resolve() == p and stat.S_ISREG(m.st_mode) and m.st_nlink == 1
                and m.st_uid == 0 and not m.st_mode & 0o022 and os.access(p, os.X_OK)):
            return str(p)
    raise Failure('trusted_postgresql_client_required')


def sql(client: str, env: dict[str, str], statement: str, rejected: bool = False) -> str:
    code, out, err = bounded([client, '-X', '--no-password', '-qAt', '-v', 'ON_ERROR_STOP=1',
                              '-v', 'VERBOSITY=verbose', '-f', '-'], env, statement)
    if rejected:
        need(code != 0 and re.search(r'\b42501\b', err) is not None, 'permission_denial_not_proven')
    else:
        need(code == 0, 'postgres_statement_failed')
    return out.strip()


def fixture() -> tuple[dict[str, str], dict, dict]:
    identity = str(uuid.uuid4())
    expected = {'delivery_id': identity, 'event_id': '$cli-' + identity,
                'sender': '@cli-fixture:example', 'room_id': '!cli-fixture:example'}
    payload = {'event_id': expected['event_id'], 'event_type': 'm.room.message',
               'room_id': expected['room_id'], 'sender': expected['sender'],
               'text': 'CLI database fixture', 'content': {'msgtype': 'm.text', 'body': 'CLI database fixture'},
               'timestamp_ms': 1789000000000, 'metadata': {'upstream': 'matrix-bot-relay'}}
    expected['payload_sha256'] = digest(wire(payload))
    expected['request_fingerprint'] = fingerprint(expected)
    binding = {'schema': 'cex.matrix.delivery-binding.v1', 'source': 'matrix-bot-relay-headers-v1',
               'delivery_id': identity, 'payload_sha256': expected['payload_sha256'],
               'event_id': expected['event_id'], 'room_id': expected['room_id'],
               'matrix_user_id': expected['sender'], 'request_fingerprint': expected['request_fingerprint']}
    task = 'task-' + identity
    forwarded = {'task_id': task, 'consumer_status': 'received',
                 'raw': {'invocation_id': task, 'status': 'accepted'},
                 'source': {'kind': 'matrix_message', 'event_id': expected['event_id'],
                            'room_id': expected['room_id'], 'matrix_user_id': expected['sender'],
                            'identity_scope': {'user_id': expected['sender'], 'room_id': expected['room_id']},
                            'metadata': {'event_type': payload['event_type'], 'timestamp_ms': payload['timestamp_ms'],
                                         'content': payload['content'],
                                         'metadata': {'upstream': 'matrix-bot-relay', 'cex_delivery_binding': binding}}}}
    envelope = {'accepted': True, 'action': 'task_result_reconciled', **expected,
                'forwarded': forwarded, 'projected_reply': None,
                'production_authorization': 'not_granted', 'result_delivery_binding': copy.deepcopy(binding),
                'reconciliation': {'schema': 'cex.matrix.adapter-result-reconciliation.v2',
                                   'source': 'consumer_entry_durable_replay', 'read_only': True,
                                   'causal_binding': 'delivery_payload_fingerprint'}}
    return expected, payload, envelope


def mutants(envelope: dict) -> list[tuple[str, dict]]:
    output = []
    for key in ('delivery_id', 'payload_sha256', 'event_id', 'room_id', 'sender', 'request_fingerprint'):
        value = copy.deepcopy(envelope); value[key] = 'changed-' + value[key]
        output.append(('outer-' + key, value))
    for key in envelope['result_delivery_binding']:
        value = copy.deepcopy(envelope)
        value['forwarded']['source']['metadata']['metadata']['cex_delivery_binding'][key] = 'changed'
        output.append(('embedded-' + key, value))
    for name in ('missing-raw', 'missing-invocation', 'changed-invocation', 'empty-task', 'extra-binding-field'):
        value = copy.deepcopy(envelope)
        if name == 'missing-raw':
            value['forwarded'].pop('raw')
        elif name == 'missing-invocation':
            value['forwarded']['raw'].pop('invocation_id')
        elif name == 'changed-invocation':
            value['forwarded']['raw']['invocation_id'] = 'wrong-task'
        elif name == 'empty-task':
            value['forwarded']['task_id'] = ''; value['forwarded']['raw']['invocation_id'] = ''
        else:
            value['forwarded']['source']['metadata']['metadata']['cex_delivery_binding']['extra'] = 'forbidden'
        output.append((name, value))
    return output


class LookupFixture(ThreadingHTTPServer):
    daemon_threads = True
    block_on_close = True
    def __init__(self, expected: dict, token: str, envelope: dict):
        self.expected, self.token, self.envelope = expected, token, envelope
        self.requests = 0
        self.invalid_requests = 0
        super().__init__(('127.0.0.1', 0), LookupHandler)
    def handle_error(self, request, client_address) -> None:
        self.invalid_requests += 1


class LookupHandler(BaseHTTPRequestHandler):
    def log_message(self, format: str, *args: object) -> None:
        pass
    def do_POST(self) -> None:
        self.connection.settimeout(5)
        server = self.server
        try:
            need(self.path == '/v1/matrix/results/lookup'
                 and self.headers.get('x-entry-token') == server.token, 'invalid_lookup_request')
            size = int(self.headers.get('Content-Length', '0'))
            need(0 < size < 8192, 'lookup_body_limit')
            need(json.loads(self.rfile.read(size)) == server.expected, 'lookup_request_binding_mismatch')
            server.requests += 1
            value = copy.deepcopy(server.envelope)
            value['generated_at'] = datetime.now(timezone.utc).isoformat()
            value['fixture_observation'] = server.requests
            data = wire(value)
            self.send_response(200); self.send_header('Content-Type', 'application/json')
            self.send_header('Content-Length', str(len(data))); self.end_headers(); self.wfile.write(data)
        except Exception:
            server.invalid_requests += 1
            self.send_error(400)


def snapshot(client: str, env: dict[str, str], e: dict) -> dict:
    delivery, event = text_sql(e['delivery_id']), text_sql(e['event_id'])
    return json.loads(sql(client, env, f"""select json_build_object(
      'outbox',(select row_to_json(o) from public.matrix_transport_outbox o where delivery_id=({delivery})::uuid),
      'history',(select count(*) from public.matrix_transport_delivery_history where delivery_id=({delivery})::uuid and from_status='dead_letter' and to_status='sent'),
      'observations',(select count(*) from public.matrix_transport_adapter_result_observations where delivery_id=({delivery})::uuid),
      'source_deliveries',(select count(*) from public.matrix_transport_outbox where source_event_id={event}));"""))


def execute() -> dict:
    need(os.name == 'posix', 'linux_database_test_required')
    owner = config(dict(os.environ))
    client = client_path()
    source = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip()
    tree = subprocess.check_output(['git', 'rev-parse', 'HEAD^{tree}'], cwd=ROOT, text=True).strip()
    need(re.fullmatch('[0-9a-f]{40}', source) is not None, 'exact_git_source_required')
    need(not subprocess.check_output(['git', 'status', '--porcelain', '--untracked-files=no'], cwd=ROOT), 'dirty_source')
    hashes = {p: digest((ROOT / p).read_bytes()) for p in SOURCES}
    observed = sql(client, owner, "select current_database() || ':' || current_setting('server_version_num');")
    need(re.fullmatch(r'matrix_review_ci:16\d{4}', observed) is not None, 'wrong_database_or_server')
    for function in FUNCTIONS:
        need(sql(client, owner, f"select count(*) from pg_proc p join pg_namespace n on n.oid=p.pronamespace where n.nspname='public' and proname='{function}';") == '1', 'complete_migration_chain_required')
    e, payload, envelope = fixture()
    login = 'cex_cli_test_' + secrets.token_hex(8)
    password, token = secrets.token_hex(24), secrets.token_hex(24)
    runtime = dict(owner, PGUSER=login, PGPASSWORD=password)
    created = False
    cases: list[str] = []
    try:
        sql(client, owner, f"begin; create role {login} login password '{password}' nosuperuser nocreatedb nocreaterole noreplication nobypassrls inherit; grant cex_matrix_reconciler_runtime to {login}; commit;")
        created = True
        need(sql(client, runtime, 'select current_user;') == login, 'runtime_identity_mismatch')
        need(sql(client, owner, f"select rolsuper or rolcreatedb or rolcreaterole or rolreplication or rolbypassrls from pg_roles where rolname='{login}';") == 'f', 'runtime_role_elevated')
        sql(client, owner, f"""begin;
select public.cex_matrix_accept_source_event_v1({text_sql(e['event_id'])}, {text_sql(digest(wire(payload)))}, 'cli-process-test', 'cli-process-cursor');
select public.cex_matrix_enqueue_delivery_v1(({text_sql(e['delivery_id'])})::uuid, {text_sql(e['event_id'])}, 'matrix-relay-adapter-v1', {text_sql(e['payload_sha256'])}, ({text_sql(wire(payload).decode())})::jsonb, 3);
update public.matrix_transport_outbox set status='dead_letter', last_error_code='adapter_response_unknown_network', lease_owner=null, lease_expires_at=null, sent_at=null, updated_at=clock_timestamp() where delivery_id=({text_sql(e['delivery_id'])})::uuid;
commit;""")
        for function in FUNCTIONS[:2]:
            types = sql(client, owner, f"select oidvectortypes(proargtypes) from pg_proc p join pg_namespace n on n.oid=p.pronamespace where n.nspname='public' and proname='{function}';").split(', ')
            need(all(t in ('uuid', 'text', 'jsonb') for t in types), 'unexpected_function_signature')
            sql(client, runtime, f"select public.{function}(" + ','.join('null::' + t for t in types) + ');', rejected=True)
            cases.append('runtime-denies-' + function)
        for statement in ('update public.matrix_transport_outbox set status=status where false;',
                          'delete from public.matrix_transport_outbox where false;',
                          'truncate public.matrix_transport_outbox;'):
            # Abort even if a regression accidentally grants the mutation.
            sql(client, runtime, 'begin; ' + statement + ' rollback;', rejected=True)
        cases.append('runtime-denies-direct-dml-and-truncate')
        baseline = snapshot(client, owner, e)
        need(baseline['outbox']['status'] == 'dead_letter' and baseline['history'] == 0
             and baseline['observations'] == 0 and baseline['source_deliveries'] == 1, 'fixture_setup_failed')
        child_env = {'PATH': '/usr/bin:/bin', 'LANG': 'C.UTF-8', 'HOME': '/nonexistent',
                     'MATRIX_ENTRY_INGRESS_TOKEN': token,
                     'MATRIX_RECONCILIATION_DATABASE_URL': f"postgresql://{login}:{password}@127.0.0.1:{owner['PGPORT']}/matrix_review_ci",
                     'MATRIX_RECONCILIATION_PSQL': client,
                     'PGSERVICE': 'must_not_be_used', 'PGSERVICEFILE': '/nonexistent',
                     'PGHOSTADDR': '127.0.0.2', 'PGUSER': 'must_not_be_used',
                     'PGDATABASE': 'must_not_be_used', 'PGOPTIONS': '-c invalid_setting=1'}
        with LookupFixture(e, token, envelope) as server:
            thread = threading.Thread(target=server.serve_forever, daemon=True); thread.start()
            args = [sys.executable, '-B', str(ROOT / CLI), '--adapter-base-url',
                    f'http://127.0.0.1:{server.server_port}', '--candidate-sha', source,
                    '--allow-http-loopback', '--allow-insecure-database-loopback',
                    '--timeout-seconds', '5', '--psql-timeout-seconds', '15']
            for key in ('delivery_id', 'event_id', 'room_id', 'sender', 'payload_sha256'):
                args.extend(['--' + key.replace('_', '-'), e[key]])
            def call(value: dict, success: bool) -> dict | None:
                server.envelope = value
                before = server.requests
                code, out, err = bounded(args, child_env)
                need(server.invalid_requests == 0 and server.requests == before + 1, 'unexpected_http_side_effect')
                need(all(secret not in out + err for secret in (password, token, child_env['MATRIX_RECONCILIATION_DATABASE_URL'])), 'secret_exposure')
                if not success:
                    need(code != 0, 'hostile_response_accepted')
                    return None
                need(code == 0, 'real_cli_failed')
                result = json.loads(out)
                need(result.get('status') == 'ok' and result.get('security_contract') == 'v3'
                     and result.get('schema') == 'cex.matrix.adapter-result-reconciler.v3'
                     and result.get('candidate_sha') == source
                     and result.get('production_authorization') == 'not_granted', 'cli_result_contract_mismatch')
                return result
            try:
                for name, hostile in mutants(envelope):
                    call(hostile, False)
                    need(snapshot(client, owner, e) == baseline, 'hostile_response_changed_database')
                    cases.append(name)
                first = call(envelope, True); retry = call(envelope, True)
                need(first['disposition'] == 'reconciled' and retry['disposition'] == 'replay', 'cli_replay_disposition')
                need(first['lookup_response_sha256'] != retry['lookup_response_sha256'], 'observation_not_fresh')
                after = snapshot(client, owner, e)
                need(after['outbox']['status'] == 'sent' and after['history'] == 1
                     and after['observations'] == 2 and after['source_deliveries'] == 1, 'cli_replay_database_invariant')
                cases.extend(['real-cli-reconciled', 'fresh-response-replay-one-transition', 'closed-pg-environment'])
                changed = copy.deepcopy(envelope)
                changed['forwarded']['raw']['status'] = 'altered-after-terminal'
                call(changed, False)
                need(snapshot(client, owner, e) == after, 'terminal_collision_changed_database')
                cases.append('terminal-result-collision')
            finally:
                server.shutdown(); thread.join(timeout=5)
                need(not thread.is_alive(), 'http_fixture_shutdown_failed')
        need({p: digest((ROOT / p).read_bytes()) for p in SOURCES} == hashes, 'source_changed')
        return {'schema': 'cex.matrix-cli-postgres-regression.v1', 'status': 'ok',
                'source_sha': source, 'source_tree': tree, 'source_sha256': hashes,
                'server': observed, 'executed_cases': cases, 'case_count': len(cases),
                'http_authority': 'synthetic-loopback-fixture', 'database_and_cli': 'real',
                'scope': 'disposable-database-cli-regression-only', 'production_authorization': 'not_granted'}
    finally:
        if created:
            sql(client, owner, f'revoke cex_matrix_reconciler_runtime from {login}; drop role {login};')


class UnitTests(unittest.TestCase):
    def test_disposable_config(self):
        env = {'MATRIX_TEST_ALLOW_SCHEMA_RESET': '1', 'MATRIX_CLI_TEST_ALLOW_ROLE_CREATE': '1',
               'MATRIX_TEST_DATABASE_URL': 'postgresql://cex:synthetic@127.0.0.1:5432/matrix_review_ci'}
        self.assertEqual(config(env)['PGHOST'], '127.0.0.1')
        for key in ('MATRIX_TEST_ALLOW_SCHEMA_RESET', 'MATRIX_CLI_TEST_ALLOW_ROLE_CREATE'):
            with self.assertRaises(Failure): config(dict(env, **{key: '0'}))
        for url in ('postgresql://cex:synthetic@db.example/matrix_review_ci',
                    'postgresql://cex:synthetic@localhost/matrix_review_ci',
                    'postgresql://cex:synthetic@127.0.0.1/production',
                    'postgresql://cex:@127.0.0.1/matrix_review_ci',
                    'postgresql://cex:synthetic@127.0.0.1/matrix_review_ci?sslmode=disable',
                    'postgresql://cex:synthetic@127.0.0.1:bad/matrix_review_ci'):
            with self.subTest(url=url), self.assertRaises(Failure): config(dict(env, MATRIX_TEST_DATABASE_URL=url))
    def test_independent_fingerprint(self):
        e, payload, envelope = fixture()
        self.assertEqual(e['payload_sha256'], digest(wire(payload)))
        self.assertEqual(e['request_fingerprint'], fingerprint(e))
        for key in ('delivery_id', 'payload_sha256', 'event_id', 'sender', 'room_id'):
            self.assertNotEqual(fingerprint(e), fingerprint(dict(e, **{key: e[key] + 'x'})))
        self.assertEqual(envelope['forwarded']['task_id'], envelope['forwarded']['raw']['invocation_id'])
    def test_mutation_isolation(self):
        _, _, envelope = fixture(); old = wire(envelope)
        mutations = mutants(envelope)
        self.assertEqual(len(mutations), 19)
        self.assertEqual(len({name for name, _ in mutations}), 19)
        self.assertEqual(wire(envelope), old)
        for _, value in mutations:
            self.assertNotEqual(wire(value), old)
    def test_sql_encoding(self):
        hostile = "x'); drop table x; --\n雪"
        self.assertNotIn(hostile, text_sql(hostile))
        encoded = text_sql(hostile).split("'")[1]
        self.assertEqual(base64.b64decode(encoded).decode(), hostile)
    def test_json_finiteness(self):
        with self.assertRaises(ValueError): wire({'value': float('nan')})

    def test_subprocess_boundaries(self):
        code, out, err = bounded([sys.executable, '-c', "print('bounded-test')"], {})
        self.assertEqual((code, out.strip(), err), (0, 'bounded-test', ''))
        with self.assertRaises(Failure):
            bounded([sys.executable, '-c', 'import time; time.sleep(10)'], {}, timeout=1)
    def test_http_fixture(self):
        from urllib.request import Request, urlopen
        from urllib.error import HTTPError
        e, _, envelope = fixture()
        token = 'unit-fixture-token'
        with LookupFixture(e, token, envelope) as server:
            thread = threading.Thread(target=server.serve_forever, daemon=True)
            thread.start()
            url = f'http://127.0.0.1:{server.server_port}/v1/matrix/results/lookup'
            try:
                values = []
                for _ in range(2):
                    request = Request(url, data=wire(e), headers={'x-entry-token': token})
                    with urlopen(request, timeout=3) as response:
                        values.append(json.loads(response.read()))
                self.assertEqual(values[0]['forwarded'], values[1]['forwarded'])
                self.assertNotEqual(wire(values[0]), wire(values[1]))
                with self.assertRaises(HTTPError):
                    urlopen(Request(url, data=wire(e)), timeout=3)
                self.assertEqual(server.requests, 2)
                self.assertEqual(server.invalid_requests, 1)
            finally:
                server.shutdown(); thread.join(timeout=5)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--self-test', action='store_true')
    args = parser.parse_args()
    if args.self_test:
        result = unittest.TextTestRunner(verbosity=2).run(unittest.defaultTestLoader.loadTestsFromTestCase(UnitTests))
        return 0 if result.wasSuccessful() else 1
    try:
        result = execute()
    except Exception as error:
        result = {'schema': 'cex.matrix-cli-postgres-regression.v1', 'status': 'failed',
                  'problem': str(error) if isinstance(error, Failure) else 'cli_regression_unavailable',
                  'production_authorization': 'not_granted'}
    print(json.dumps(result, sort_keys=True, indent=2))
    return 0 if result['status'] == 'ok' else 1


if __name__ == '__main__':
    raise SystemExit(main())
