#!/usr/bin/env python3
"""Real process/filesystem tests with explicitly fake psql; no SQL execution."""
from __future__ import annotations
import contextlib
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import time
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location('matrix_runner_hardened', ROOT/'scripts/matrix_postgres_regression.py')
assert SPEC and SPEC.loader
R = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(R)
BASE = "DO $test$ begin raise exception 'matrix_source_event_identity_collision'; end; $test$;\n"


class Fixture(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)/'source'
        self.root.mkdir()
        for name in R.MIGRATIONS:
            self.write('services/matrix-entry-adapter/migrations/'+name, 'begin;\n-- '+name+'\ncommit;\n')
        self.write(R.BASELINE, BASE)
        for name in R.REGRESSIONS:
            self.write(name, 'begin; select 1; rollback;\n')
        for name in R.ENTRYPOINTS:
            self.write(name, R.wrapper_source())
        self.write('scripts/matrix_postgres_regression.py', Path(R.__file__).read_text())
        self.tools = Path(self.temp.name)/'bin'
        self.tools.mkdir()
        self.trace = Path(self.temp.name)/'client-called'
        self.env = dict(os.environ, MATRIX_TEST_DATABASE_URL='postgres://u:PRIVATE_SECRET@localhost/matrix_test_ci',
                        MATRIX_TEST_ALLOW_SCHEMA_RESET='1', PATH=str(self.tools)+os.pathsep+os.environ['PATH'])

    def write(self, relative, text):
        p=self.root/relative
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text(text)
        return p

    def fake_client(self, mode='success'):
        body = f'''#!{sys.executable}
# Deliberately fake: produces a transcript without contacting PostgreSQL.
import pathlib,re,sys
pathlib.Path({str(self.trace)!r}).write_text("called")
text=sys.stdin.read()
mode={mode!r}
if mode == "silent": sys.exit(0)
if mode == "error":
    print("PRIVATE_SECRET private payload",file=sys.stderr)
    sys.exit(3)
for line in text.splitlines():
    if line.startswith("\\\\echo "):
        print(line[6:])
    elif line.startswith("select 'CEX_MATRIX_"):
        print(re.search(r"'(CEX_MATRIX_[a-f0-9]+:version:)'",line).group(1)+"160015")
'''
        p=self.tools/'psql'; p.write_text(body); p.chmod(0o755)

    def cli(self, wrapper=None, args=(), **env):
        command = ['bash',str(self.root/(wrapper or R.ENTRYPOINTS[0])),*args]
        return subprocess.run(command,env={**self.env,**env},capture_output=True,text=True,timeout=10)


class EntryPointTests(Fixture):
    def test_both_entrypoints_are_byte_identical_and_safe(self):
        for name in R.ENTRYPOINTS:
            content=(ROOT/name).read_text()
            self.assertEqual(content,R.wrapper_source())
            for forbidden in ['eval ', 'psql ', 'truncate ', '0001_transport_durability.sql', 'status":"ok']:
                self.assertNotIn(forbidden,content)

    def test_original_assertions_moved_without_one_byte_change(self):
        value=(ROOT/R.BASELINE).read_bytes()
        self.assertEqual(len(value),10870)
        self.assertEqual(hashlib.sha256(value).hexdigest(),
                         '4a773ac20af7115ee5f4fbfdafa484badbea3a34be04b6debae5e69ddd214dd3')

    def test_both_entrypoints_reject_missing_consent_before_client(self):
        self.fake_client()
        for wrapper in R.ENTRYPOINTS:
            result=self.cli(wrapper,MATRIX_TEST_ALLOW_SCHEMA_RESET='0')
            self.assertNotEqual(result.returncode,0)
            self.assertFalse(self.trace.exists())
            self.assertNotIn('Traceback',result.stderr)
            self.assertNotIn('PRIVATE_SECRET',result.stdout+result.stderr)

    def test_both_entrypoints_reject_production_database_before_client(self):
        self.fake_client()
        for wrapper in R.ENTRYPOINTS:
            result=self.cli(wrapper,MATRIX_TEST_DATABASE_URL='postgres://u:PRIVATE_SECRET@localhost/production')
            self.assertNotEqual(result.returncode,0)
            self.assertFalse(self.trace.exists())
            self.assertNotIn('Traceback',result.stderr)

    def test_old_entrypoint_cannot_run_only_0001(self):
        self.fake_client()
        (self.root/'services/matrix-entry-adapter/migrations'/R.MIGRATIONS[-1]).unlink()
        result=self.cli()
        self.assertNotEqual(result.returncode,0)
        self.assertFalse(self.trace.exists())

    def test_both_entrypoints_execute_the_real_single_process_driver(self):
        self.fake_client()
        for wrapper in R.ENTRYPOINTS:
            result=self.cli(wrapper)
            self.assertEqual(result.returncode,0,result.stderr+result.stdout)
            self.assertTrue(self.trace.exists())
            record=json.loads(result.stdout)
            self.assertEqual(record['execution_model'],'single_psql_session')
            self.assertEqual(len(record['stages']),3 + 2 * len(R.MIGRATIONS) + 1 + len(R.REGRESSIONS))
            self.assertEqual(record['production_authorization'],'not_granted')
            self.trace.unlink()

    def test_silent_successful_client_is_not_sql_success(self):
        self.fake_client('silent')
        result=self.cli()
        self.assertNotEqual(result.returncode,0)
        self.assertEqual(json.loads(result.stdout)['status'],'failed')

    def test_error_diagnostics_do_not_leak_through_wrappers(self):
        self.fake_client('error')
        result=self.cli()
        self.assertNotEqual(result.returncode,0)
        self.assertNotIn('PRIVATE_SECRET',result.stdout+result.stderr)
        self.assertNotIn('private payload',result.stdout+result.stderr)

    def test_bad_output_path_is_rejected_before_client(self):
        self.fake_client()
        victim=self.write('docs/status.json','unchanged')
        result=self.cli(args=('--evidence',str(victim)))
        self.assertEqual(result.returncode,1)
        self.assertFalse(self.trace.exists())
        self.assertNotIn('Traceback',result.stderr)
        self.assertEqual(result.stderr.strip(),'invalid evidence output; no database operation attempted')
        self.assertEqual(victim.read_text(),'unchanged')

    def test_external_and_parent_escape_paths_do_not_run_client(self):
        self.fake_client()
        for output in [str(Path(self.temp.name)/'out.json'),'run/../docs/out.json','run/result.txt']:
            result=self.cli(args=('--evidence',output))
            self.assertNotEqual(result.returncode,0)
            self.assertFalse(self.trace.exists())
            self.assertNotIn('Traceback',result.stderr)

    def test_symbolic_report_directory_does_not_run_client(self):
        self.fake_client()
        (self.root/'run').symlink_to(self.tools,target_is_directory=True)
        result=self.cli(args=('--evidence','run/out.json'))
        self.assertNotEqual(result.returncode,0)
        self.assertFalse(self.trace.exists())
        self.assertFalse((self.tools/'out.json').exists())

    def test_failed_attempt_replaces_stale_success_with_failure_report(self):
        self.fake_client('silent')
        previous=self.write('run/report.json','{"status":"ok"}')
        result=self.cli(args=('--evidence','run/report.json'))
        self.assertNotEqual(result.returncode,0)
        self.assertEqual(json.loads(previous.read_text())['status'],'failed')
        self.assertEqual(list(previous.parent.glob('.matrix-evidence-*.tmp')),[])

    def test_unknown_arguments_cannot_run_client(self):
        self.fake_client()
        for args in [('--allow-production',),('--evidence',)]:
            result=self.cli(args=args)
            self.assertNotEqual(result.returncode,0)
            self.assertFalse(self.trace.exists())
            self.assertNotIn('Traceback',result.stderr)


class ConnectionAndSnapshotTests(Fixture):
    def test_single_host_required_not_libpq_failover_list(self):
        for host in ['one,two','one%2Ctwo','%2ftmp','a b','-a','a..b','a/other','[fe80::1%25eth0]']:
            with self.subTest(host=host),self.assertRaises(R.RegressionError):
                R.database_environment({**self.env,'MATRIX_TEST_DATABASE_URL':f'postgres://u:p@{host}/matrix_test_ci'})

    def test_ipv4_ipv6_and_one_dns_name_are_allowed(self):
        for host in ['127.0.0.1','[::1]','postgres','matrix.example.']:
            self.assertTrue(R.database_environment({**self.env,'MATRIX_TEST_DATABASE_URL':f'postgres://u:p@{host}/matrix_test_ci'})['PGHOST'])

    def test_encoded_controls_and_noncanonical_timeout_are_rejected(self):
        for url in ['postgres://u:p%09x@localhost/matrix_test_ci','postgres://u:p%7fx@localhost/matrix_test_ci',
                    'postgres://u:p@localhost/matrix_test_ci?connect_timeout=01',
                    'postgres://u:p@localhost/matrix_test_ci?connect_timeout=\u0665']:
            with self.assertRaises(R.RegressionError): R.database_environment({**self.env,'MATRIX_TEST_DATABASE_URL':url})

    def test_source_parent_symlink_is_rejected(self):
        original=self.root/'services'; other=original.with_name('actual-services'); original.rename(other)
        original.symlink_to(other,target_is_directory=True)
        with self.assertRaises(R.RegressionError): R.input_stages(self.root)

    def test_hardlinked_source_is_rejected(self):
        os.link(self.root/R.BASELINE,Path(self.temp.name)/'linked.sql')
        with self.assertRaises(R.RegressionError): R.input_stages(self.root)

    def test_fifo_source_is_rejected_without_read_blocking(self):
        path=self.root/R.BASELINE;path.unlink();os.mkfifo(path)
        with self.assertRaises(R.RegressionError):R.input_stages(self.root)

    def test_oversized_source_is_rejected(self):
        with patch.object(R,'MAX_INPUT',32),self.assertRaises(R.RegressionError):R.input_stages(self.root)

    def test_reconnect_and_error_waiver_metacommands_are_rejected(self):
        for command in ['\\connect production','\\set ON_ERROR_STOP off','\\! touch /tmp/private']:
            self.write(R.REGRESSIONS[0],command+'\n')
            with self.assertRaises(R.RegressionError):R.input_stages(self.root)

    def test_old_unsafe_wrapper_cannot_be_reintroduced(self):
        self.write(R.ENTRYPOINTS[0],'#!/bin/bash\npsql -f old.sql\n')
        with self.assertRaises(R.RegressionError):R.input_stages(self.root)

    def test_allowlist_is_exact_and_reset_uses_no_prefix_wildcard(self):
        self.assertEqual(len(R.TRANSPORT_TABLES),12)
        self.assertIn('matrix_transport_stream_scopes',R.TRANSPORT_TABLES)
        self.assertNotIn("like 'matrix",R.FOREIGN_TABLES_SQL)
        self.assertNotIn("like 'matrix",R.RESET_SQL)
        self.assertIn('matrix_test_unrelated_objects',R.RESET_SQL)
        self.assertIn("c.relkind = 'r'",R.FOREIGN_TABLES_SQL)

    def test_source_drift_cannot_be_reported_as_success(self):
        def mutate(argv,**kwargs):
            record=fake_result(argv,kwargs['input'])
            self.write(R.REGRESSIONS[0],'begin; select 2; rollback;\n')
            return record
        with patch.object(R.shutil,'which',return_value='/fake/psql'):
            self.assertEqual(R.execute(self.root,self.env,mutate)['status'],'failed')

    def test_new_migration_during_execution_invalidates_report(self):
        def mutate(argv,**kwargs):
            record=fake_result(argv,kwargs['input'])
            self.write('services/matrix-entry-adapter/migrations/0005_unreviewed.sql','begin; commit;')
            return record
        with patch.object(R.shutil,'which',return_value='/fake/psql'):
            self.assertEqual(R.execute(self.root,self.env,mutate)['status'],'failed')


def fake_result(argv,script):
    lines=[]
    for line in script.splitlines():
        if line.startswith('\\echo '): lines.append(line[6:])
        elif line.startswith("select 'CEX_MATRIX_"):
            lines.append(re.search(r"'(CEX_MATRIX_[a-f0-9]+:version:)'",line).group(1)+'160015')
    return subprocess.CompletedProcess(argv,0,'\n'.join(lines)+'\n','')


class ObservationAndProcessTests(unittest.TestCase):
    def setUp(self):
        self.nonce='a'*32
        self.script,self.names=R.session_sql('matrix_test_ci',[('test-one','select 1;')],self.nonce)
        self.output=fake_result([],self.script).stdout

    def test_guards_reset_and_checks_use_one_session_script(self):
        self.assertIn('pg_try_advisory_lock',self.script)
        self.assertIn("current_database() <> 'matrix_test_ci'",self.script)
        self.assertNotIn('\\connect',self.script)
        self.assertLess(self.script.index('matrix_test_runner_busy'),self.script.index(':start:reset-transport-test-rows'))
        self.assertLess(self.script.index('matrix_test_unrelated_objects'),self.script.index('truncate table'))

    def test_complete_ordered_transcript_is_required(self):
        records,version,valid=R.parse_observation(self.output,self.names,self.nonce)
        self.assertTrue(valid);self.assertEqual(version,160015)
        self.assertTrue(all(r['status']=='passed' for r in records))
        for value in ['',self.output.replace(':ok:test-one',':ok:other'),self.output+self.output,
                      self.output.replace('a'*32,'b'*32),self.output.replace(':version:160015',':version:170000')]:
            self.assertFalse(R.parse_observation(value,self.names,self.nonce)[2])

    def test_unrelated_success_text_is_not_stage_completion(self):
        self.assertFalse(R.parse_observation('all tests passed\n{"status":"ok"}',self.names,self.nonce)[2])

    def test_partial_transcript_retains_only_observed_progress(self):
        output=self.output.rsplit('CEX_MATRIX_',1)[0]
        records,_,valid=R.parse_observation(output,self.names,self.nonce)
        self.assertFalse(valid)
        self.assertEqual(records[-1]['status'],'not_completed')

    def test_real_child_process_is_executed_and_stderr_is_not_echoed(self):
        result=R.bounded_client([sys.executable,'-c',"import sys;print('hello');print('PRIVATE_SECRET',file=sys.stderr)"],input='',env=dict(os.environ),timeout=2)
        self.assertEqual(result.stdout,'hello\n');self.assertEqual(result.stderr,'')

    def test_real_child_timeout_cannot_be_success(self):
        started=time.monotonic()
        with self.assertRaisesRegex(R.RegressionError,'timeout'):
            R.bounded_client([sys.executable,'-c','import time;time.sleep(10)'],input='',env=dict(os.environ),timeout=0.1)
        self.assertLess(time.monotonic()-started,3)

    def test_real_child_output_limit_is_enforced(self):
        with self.assertRaisesRegex(R.RegressionError,'output_limit'):
            R.bounded_client([sys.executable,'-c',"import sys;sys.stdout.write('x'*2097152)"],input='',env=dict(os.environ),timeout=2)

    def test_atomic_report_failure_preserves_previous_bytes(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);(root/'run').mkdir();out=root/'run/out.json';out.write_text('old')
            with patch.object(R.os,'replace',side_effect=OSError('fixture')),self.assertRaises(OSError):
                R.write_evidence(root,out,'new')
            self.assertEqual(out.read_text(),'old')
            self.assertEqual(list(out.parent.glob('.matrix-evidence-*.tmp')),[])
            # Exercise the CLI failure diagnostic as well as the atomic writer.
            with patch.object(R,'ROOT',root), patch.object(R,'execute',return_value={'status':'ok'}), \
                    patch.object(R,'write_evidence',side_effect=OSError('private failure')), \
                    patch.object(sys,'argv',['runner','--evidence','run/out.json']), \
                    contextlib.redirect_stderr(io.StringIO()) as errors:
                self.assertEqual(R.main(),1)
            self.assertEqual(errors.getvalue().strip(),'evidence publication failed; no qualification granted')
            self.assertNotIn('private failure',errors.getvalue())


if __name__=='__main__':
    unittest.main(verbosity=2)
