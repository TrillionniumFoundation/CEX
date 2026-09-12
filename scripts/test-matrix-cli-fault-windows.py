#!/usr/bin/env python3
"""Real CLI/libpq fault and concurrency tests on the consented disposable database.

The HTTP lookup remains synthetic. This tests PostgreSQL response loss and row
locking, not a real homeserver, external Agent, remote TLS or production recovery.
"""
from __future__ import annotations

from concurrent.futures import ThreadPoolExecutor
import copy
from datetime import datetime, timezone
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import secrets
import signal
import socket
import subprocess
import sys
import tempfile
import threading
import time
import unittest

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location('matrix_cli_regression_base', ROOT / 'scripts/test-matrix-cli-postgres.py')
if SPEC is None or SPEC.loader is None:
    raise RuntimeError('matrix_cli_regression_base_missing')
base = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(base)
need = base.need
MAX_WIRE = 1024 * 1024
MATCH = b'public.cex_matrix_reconcile_adapter_result_v3('


def exact(sock: socket.socket, count: int) -> bytes:
    need(0 <= count <= MAX_WIRE, 'proxy_frame_limit')
    result = bytearray()
    while len(result) < count:
        part = sock.recv(count - len(result))
        if not part:
            raise EOFError()
        result.extend(part)
    return bytes(result)


def frame(sock: socket.socket) -> tuple[bytes, bytes, bytes]:
    kind = exact(sock, 1)
    length = exact(sock, 4)
    size = int.from_bytes(length, 'big')
    need(4 <= size <= MAX_WIRE, 'proxy_frame_limit')
    body = exact(sock, size - 4)
    return kind, body, kind + length + body


def stop(sock: socket.socket | None) -> None:
    if sock is not None:
        try:
            sock.shutdown(socket.SHUT_RDWR)
        except OSError:
            pass
        sock.close()


class ResponseLossProxy:
    """One loopback connection; never logs or retains credentials/query bytes."""
    def __init__(self, port: int, mode: str):
        need(mode in ('before_query', 'after_commit') and 1 <= port <= 65535, 'proxy_config_invalid')
        self.port, self.mode = port, mode
        self.query_seen = threading.Event()
        self.dropped = threading.Event()
        self.idle_after_query = threading.Event()
        self.failed = False
        self.front = self.back = None
        self.listener = socket.socket()
        self.listener.bind(('127.0.0.1', 0)); self.listener.listen(1)
        self.listener.settimeout(15)
        self.local_port = self.listener.getsockname()[1]
        self.thread = threading.Thread(target=self.run, daemon=True)

    def __enter__(self):
        self.thread.start()
        return self

    def frontend(self) -> None:
        try:
            while True:
                kind, body, raw = frame(self.front)
                if kind == b'Q' and MATCH in body:
                    need(body.count(MATCH) == 1, 'proxy_ambiguous_query')
                    self.query_seen.set()
                    if self.mode == 'before_query':
                        self.dropped.set(); stop(self.front); stop(self.back)
                        return
                self.back.sendall(raw)
        except (EOFError, OSError):
            if not self.dropped.is_set(): self.failed = True
        except Exception:
            self.failed = True
            stop(self.front); stop(self.back)

    def run(self) -> None:
        pump = None
        try:
            self.front, address = self.listener.accept()
            need(address[0] == '127.0.0.1', 'proxy_nonlocal_client')
            self.front.settimeout(15)
            self.back = socket.create_connection(('127.0.0.1', self.port), timeout=15)
            # Startup packets are not typed frames. Reject encryption upgrades;
            # test credentials are restricted to the explicit local plaintext mode.
            for _ in range(3):
                length = exact(self.front, 4)
                size = int.from_bytes(length, 'big')
                need(8 <= size <= 8192, 'proxy_startup_limit')
                body = exact(self.front, size - 4)
                code = int.from_bytes(body[:4], 'big')
                self.back.sendall(length + body)
                if code in (80877103, 80877104):
                    response = exact(self.back, 1)
                    need(response == b'N', 'proxy_requires_unencrypted_fixture')
                    self.front.sendall(response)
                    continue
                need(code == 196608, 'proxy_startup_protocol')
                break
            else:
                raise base.Failure('proxy_startup_exhausted')
            pump = threading.Thread(target=self.frontend, daemon=True); pump.start()
            for _ in range(200):
                kind, body, raw = frame(self.back)
                if self.query_seen.is_set():
                    # Withhold the complete result, not only its trailing marker.
                    if kind == b'Z':
                        need(body == b'I' and self.mode == 'after_commit', 'proxy_expected_idle')
                        self.idle_after_query.set(); self.dropped.set()
                        stop(self.front); stop(self.back)
                        return
                else:
                    self.front.sendall(raw)
            raise base.Failure('proxy_message_limit')
        except (OSError, EOFError):
            if not self.dropped.is_set(): self.failed = True
        except Exception:
            self.failed = True
        finally:
            stop(self.front); stop(self.back)
            if pump is not None:
                pump.join(timeout=3)
                if pump.is_alive(): self.failed = True

    def __exit__(self, *args):
        stop(self.front); stop(self.back); self.listener.close()
        self.thread.join(timeout=5)
        need(not self.thread.is_alive() and not self.failed
             and self.dropped.is_set(), 'proxy_window_not_proven')


class HeldRow:
    """Keep a genuine owner row lock until both runtime sessions are waiting."""
    def __init__(self, client: str, owner: dict, delivery: str):
        self.out, self.err = tempfile.TemporaryFile(), tempfile.TemporaryFile()
        self.process = subprocess.Popen([client, '-X', '--no-password', '-qAt', '-v', 'ON_ERROR_STOP=1', '-f', '-'],
            cwd=ROOT, env=owner, stdin=subprocess.PIPE, stdout=self.out, stderr=self.err, start_new_session=True)
        self.send('begin;\nselect delivery_id from public.matrix_transport_outbox where delivery_id='
                  + base.text_sql(delivery) + "::uuid for update;\nselect 'LOCK_HELD';\n")

    def send(self, value: str) -> None:
        self.process.stdin.write(value.encode()); self.process.stdin.flush()

    def __enter__(self):
        deadline = time.monotonic() + 5
        while time.monotonic() < deadline:
            need(self.process.poll() is None, 'owner_lock_process_failed')
            if b'LOCK_HELD' in os.pread(self.out.fileno(), 8192, 0): return self
            time.sleep(0.02)
        self.close()
        raise base.Failure('owner_lock_not_observed')

    def release(self) -> None:
        self.send('commit;\n\\q\n')
        self.process.communicate(timeout=5)
        need(self.process.returncode == 0, 'owner_lock_release_failed')

    def close(self) -> None:
        if self.process.poll() is None:
            try:
                self.send('rollback;\n\\q\n'); self.process.communicate(timeout=5)
            except (OSError, subprocess.TimeoutExpired):
                os.killpg(self.process.pid, signal.SIGKILL); self.process.wait(timeout=5)
        self.out.close(); self.err.close()

    def __exit__(self, *args):
        self.close()


def seed(client: str, owner: dict, e: dict, payload: dict) -> None:
    base.sql(client, owner, f"""begin;
select public.cex_matrix_accept_source_event_v1({base.text_sql(e['event_id'])},{base.text_sql(base.digest(base.wire(payload)))},'cli-fault-test','cli-fault-cursor');
select public.cex_matrix_enqueue_delivery_v1(({base.text_sql(e['delivery_id'])})::uuid,{base.text_sql(e['event_id'])},'matrix-relay-adapter-v1',{base.text_sql(e['payload_sha256'])},({base.text_sql(base.wire(payload).decode())})::jsonb,3);
update public.matrix_transport_outbox set status='dead_letter',last_error_code='adapter_response_unknown_network',lease_owner=null,lease_expires_at=null,sent_at=null,updated_at=clock_timestamp() where delivery_id=({base.text_sql(e['delivery_id'])})::uuid;
commit;""")


def arguments(server, e: dict, source: str) -> list[str]:
    result = [sys.executable, '-B', str(ROOT/base.CLI), '--adapter-base-url',
              f'http://127.0.0.1:{server.server_port}', '--candidate-sha', source,
              '--allow-http-loopback', '--allow-insecure-database-loopback',
              '--timeout-seconds', '10', '--psql-timeout-seconds', '20']
    for key in ('delivery_id', 'event_id', 'room_id', 'sender', 'payload_sha256'):
        result += ['--' + key.replace('_', '-'), e[key]]
    return result


def result_of(response: tuple[int, str, str], sensitive: tuple[str, ...]) -> dict | None:
    code, out, err = response
    need(all(s not in out + err for s in sensitive), 'fault_test_secret_exposure')
    if code != 0:
        need(not out.strip(), 'failed_cli_claimed_output')
        return None
    value = json.loads(out)
    need(value.get('schema') == 'cex.matrix.adapter-result-reconciler.v3'
         and value.get('status') == 'ok' and value.get('production_authorization') == 'not_granted', 'fault_cli_result_invalid')
    return value


def execute() -> dict:
    need(os.name == 'posix', 'linux_database_test_required')
    owner, client = base.config(dict(os.environ)), base.client_path()
    source = subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip()
    tree = subprocess.check_output(['git','rev-parse','HEAD^{tree}'],cwd=ROOT,text=True).strip()
    need(not subprocess.check_output(['git','status','--porcelain','--untracked-files=no'],cwd=ROOT), 'dirty_source')
    paths = (*base.SOURCES, 'scripts/test-matrix-cli-fault-windows.py')
    hashes = {p: base.digest((ROOT/p).read_bytes()) for p in paths}
    server_version = base.sql(client, owner, "select current_database() || ':' || current_setting('server_version_num');")
    need(base.re.fullmatch(r'matrix_review_ci:16\d{4}', server_version) is not None, 'wrong_database_or_server')
    login, password, token = 'cex_fault_test_' + secrets.token_hex(8), secrets.token_hex(24), secrets.token_hex(24)
    made = False
    cases = []
    try:
        base.sql(client, owner, f"begin; create role {login} login password '{password}' nosuperuser nocreatedb nocreaterole noreplication nobypassrls inherit; grant cex_matrix_reconciler_runtime to {login}; commit;")
        made = True
        env = {'PATH':'/usr/bin:/bin','HOME':'/nonexistent','LANG':'C.UTF-8',
               'MATRIX_ENTRY_INGRESS_TOKEN':token,'MATRIX_RECONCILIATION_PSQL':client,
               'MATRIX_RECONCILIATION_DATABASE_URL':f"postgresql://{login}:{password}@127.0.0.1:{owner['PGPORT']}/matrix_review_ci"}
        for scenario in ('before_query', 'after_commit', 'identical_race', 'conflicting_race'):
            e,payload,envelope = base.fixture(); seed(client,owner,e,payload)
            before = base.snapshot(client,owner,e)
            need(before['outbox']['status']=='dead_letter' and before['history']==0 and before['observations']==0, 'fault_fixture_invalid')
            with base.LookupFixture(e,token,envelope) as server:
                thread=threading.Thread(target=server.serve_forever,daemon=True); thread.start()
                args=arguments(server,e,source)
                try:
                    if scenario in ('before_query','after_commit'):
                        with ResponseLossProxy(int(owner['PGPORT']),scenario) as proxy:
                            proxy_env=dict(env,MATRIX_RECONCILIATION_DATABASE_URL=f'postgresql://{login}:{password}@127.0.0.1:{proxy.local_port}/matrix_review_ci')
                            response=base.bounded(args,proxy_env,timeout=30)
                            need(result_of(response,(password,token)) is None, 'lost_response_claimed_success')
                        need(proxy.query_seen.is_set(), 'fault_query_not_seen')
                        after=base.snapshot(client,owner,e)
                        if scenario=='before_query':
                            need(after==before, 'prequery_loss_mutated_database')
                            cases.append('prequery-drop-no-mutation')
                        else:
                            need(proxy.idle_after_query.is_set() and after['outbox']['status']=='sent'
                                 and after['history']==1 and after['observations']==1, 'postcommit_window_not_proven')
                            cases.append('postcommit-response-loss-durable-state')
                        replay=result_of(base.bounded(args,env),(password,token))
                        need(replay is not None and replay['disposition']==('reconciled' if scenario=='before_query' else 'replay'), 'fault_recovery_disposition')
                        recovered=base.snapshot(client,owner,e)
                        need(recovered['history']==1 and recovered['source_deliveries']==1
                             and recovered['observations']==(1 if scenario=='before_query' else 2), 'fault_recovery_duplicate')
                        cases.append(scenario+'-retry-single-transition')
                    else:
                        alternate=copy.deepcopy(envelope)
                        if scenario=='conflicting_race': alternate['forwarded']['raw']['status']='competing-result'
                        with base.LookupFixture(e,token,alternate) as second:
                            other=threading.Thread(target=second.serve_forever,daemon=True); other.start()
                            try:
                                with HeldRow(client,owner,e['delivery_id']) as hold:
                                    with ThreadPoolExecutor(max_workers=2) as pool:
                                        futures=[pool.submit(base.bounded,a,env,timeout=30) for a in (args,arguments(second,e,source))]
                                        deadline=time.monotonic()+10
                                        while time.monotonic()<deadline:
                                            waiting=base.sql(client,owner,f"select count(*) from pg_stat_activity where usename='{login}' and wait_event_type='Lock';")
                                            if waiting=='2':break
                                            need(not any(f.done() for f in futures), 'concurrent_cli_exited_before_row_lock')
                                            time.sleep(0.05)
                                        else:raise base.Failure('two_concurrent_lock_waiters_not_proven')
                                        hold.release()
                                        results=[result_of(f.result(timeout=25),(password,token)) for f in futures]
                                after=base.snapshot(client,owner,e)
                                successes=[r for r in results if r is not None]
                                if scenario=='identical_race':
                                    need(sorted(r['disposition'] for r in successes)==['reconciled','replay']
                                         and after['observations']==2, 'identical_race_not_idempotent')
                                else:
                                    need(len(successes)==1 and successes[0]['disposition']=='reconciled'
                                         and after['observations']==1, 'conflicting_race_not_exclusive')
                                need(after['outbox']['status']=='sent' and after['history']==1
                                     and after['source_deliveries']==1, 'race_created_duplicate_effect')
                                cases.append(scenario+'-two-lock-waiters-single-transition')
                            finally:
                                second.shutdown(); other.join(timeout=5); need(not other.is_alive(),'second_lookup_shutdown')
                    need(server.invalid_requests==0,'fault_lookup_invalid_request')
                finally:
                    server.shutdown(); thread.join(timeout=5); need(not thread.is_alive(),'fault_lookup_shutdown')
        need({p:base.digest((ROOT/p).read_bytes()) for p in paths}==hashes,'source_changed')
        return {'schema':'cex.matrix-cli-fault-windows.v1','status':'ok','source_sha':source,'source_tree':tree,
                'source_sha256':hashes,'server':server_version,'executed_cases':cases,'case_count':len(cases),
                'database_cli_and_loopback_transport':'real','http_authority':'synthetic-loopback-fixture',
                'scope':'disposable-database-cli-fault-regression-only','production_authorization':'not_granted'}
    finally:
        if made: base.sql(client,owner,f'revoke cex_matrix_reconciler_runtime from {login}; drop role {login};')


class Tests(unittest.TestCase):
    def test_fragmented_frame(self):
        a,b=socket.socketpair()
        try:
            a.sendall(b'Z\0\0\0\x05I')
            self.assertEqual(frame(b),(b'Z',b'I',b'Z\0\0\0\x05I'))
        finally: a.close(); b.close()
    def test_proxy_exact_windows_with_synthetic_wire_peer(self):
        def packet(kind, body):
            return kind + (len(body)+4).to_bytes(4,'big') + body
        for mode in ('before_query','after_commit'):
            with self.subTest(mode=mode), socket.socket() as listener:
                listener.bind(('127.0.0.1',0)); listener.listen(1);listener.settimeout(5)
                forwarded=[]
                errors=[]
                def backend():
                    try:
                        connection,_=listener.accept()
                        with connection:
                            connection.settimeout(5)
                            length=int.from_bytes(exact(connection,4),'big')
                            exact(connection,length-4)
                            connection.sendall(packet(b'R',bytes(4))+packet(b'Z',b'I'))
                            try: kind,body,_=frame(connection)
                            except EOFError: return
                            forwarded.append(kind==b'Q' and MATCH in body)
                            connection.sendall(packet(b'T',b'fixture')+packet(b'D',b'fixture-result')+packet(b'C',b'SELECT 1\0')+packet(b'Z',b'I'))
                    except Exception: errors.append('synthetic_backend_failed')
                peer=threading.Thread(target=backend,daemon=True);peer.start()
                with ResponseLossProxy(listener.getsockname()[1],mode) as proxy:
                    with socket.create_connection(('127.0.0.1',proxy.local_port),timeout=5) as client:
                        startup=(196608).to_bytes(4,'big')+b'user\0fixture\0\0'
                        client.sendall((len(startup)+4).to_bytes(4,'big')+startup)
                        self.assertEqual(frame(client)[0],b'R');self.assertEqual(frame(client)[0],b'Z')
                        client.sendall(packet(b'Q',b'select '+MATCH+b'fixture);\0'))
                        self.assertEqual(client.recv(1),b'')
                peer.join(timeout=5)
                self.assertFalse(peer.is_alive()); self.assertEqual(errors,[])
                self.assertEqual(forwarded,[] if mode=='before_query' else [True])
                self.assertEqual(proxy.idle_after_query.is_set(),mode=='after_commit')
    def test_frame_budget(self):
        a,b=socket.socketpair()
        try:
            a.sendall(b'Z\xff\xff\xff\xff')
            with self.assertRaises(base.Failure):frame(b)
        finally:a.close();b.close()
    def test_proxy_config(self):
        for mode,port in [('unknown',5432),('after_commit',0)]:
            with self.assertRaises(base.Failure):ResponseLossProxy(port,mode)
    def test_source_mutant_isolation(self):
        e,p,envelope=base.fixture(); original=base.wire(envelope)
        other=copy.deepcopy(envelope);other['forwarded']['raw']['status']='different'
        self.assertEqual(base.wire(envelope),original)
        self.assertEqual(e['request_fingerprint'],base.fingerprint(e))
    def test_no_false_success(self):
        self.assertIsNone(result_of((1,'','rejected'),('secret',)))
        with self.assertRaises(base.Failure):result_of((1,'{"status":"ok"}',''),('secret',))
        with self.assertRaises(base.Failure):result_of((1,'','secret'),('secret',))


def main() -> int:
    if sys.argv[1:]==['--self-test']:
        result=unittest.TextTestRunner(verbosity=2).run(unittest.defaultTestLoader.loadTestsFromTestCase(Tests))
        return 0 if result.wasSuccessful() else 1
    try:
        need(len(sys.argv)==1,'unexpected_arguments')
        result=execute()
    except Exception as error:
        result={'schema':'cex.matrix-cli-fault-windows.v1','status':'failed',
                'problem':str(error) if isinstance(error,base.Failure) else 'fault_regression_unavailable',
                'production_authorization':'not_granted'}
    print(json.dumps(result,sort_keys=True,indent=2))
    return 0 if result['status']=='ok' else 1


if __name__=='__main__':
    raise SystemExit(main())
