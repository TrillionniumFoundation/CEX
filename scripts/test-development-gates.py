#!/usr/bin/env python3
"""Regression tests for gate acceptance, transport bounds and real doc sections.

Synthetic fixtures in these tests prove only checker behavior. They are never
hosted qualification, production evidence, or approval.
"""
from __future__ import annotations

import contextlib
import importlib.util
import io
import json
import os
import re
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]


def load(name: str, filename: str):
    spec = importlib.util.spec_from_file_location(name, ROOT / 'scripts' / filename)
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    spec.loader.exec_module(module)
    return module


gate = load('development_docs', 'check-development-docs.py')
docs = load('module_documentation', 'check-module-documentation.py')


def child(**changes):
    value = dict(schema='test.child.v1', status='ok', problems=[],
                 production_authorization='not_granted',
                 checker_may_grant_production_authorization=False)
    value.update(changes)
    return value


class GateResultTests(unittest.TestCase):
    def validate(self, value, code=0):
        return gate.validate_result(code, value, '', label='child', expected_schema='test.child.v1')

    def test_success_requires_complete_consistent_contract(self):
        self.assertEqual(self.validate(child()), [])

    def test_failed_status_cannot_hide_behind_empty_diagnostics(self):
        self.assertTrue(self.validate(child(status='failed')))

    def test_missing_status_is_not_success(self):
        value = child(); del value['status']
        self.assertTrue(self.validate(value))

    def test_nonzero_exit_with_ok_status_is_failure(self):
        self.assertTrue(self.validate(child(), 2))

    def test_nonempty_diagnostics_override_ok(self):
        self.assertTrue(self.validate(child(problems=['deliberate fixture failure'])))

    def test_malformed_diagnostics_fail_closed(self):
        for value in [None, {}, '', [None], [1], [''], [' '], ['x' * 4097], ['x'] * 201]:
            with self.subTest(value_type=type(value).__name__):
                self.assertTrue(self.validate(child(problems=value)))

    def test_wrong_schema_and_authorization_fail_closed(self):
        for updates in [dict(schema='wrong'), dict(production_authorization='granted'),
                        dict(checker_may_grant_production_authorization=True),
                        dict(checker_may_grant_production_authorization=0)]:
            with self.subTest(updates=updates):
                self.assertTrue(self.validate(child(**updates)))

    def test_absent_authorization_denial_fails_closed(self):
        value=child(); del value['checker_may_grant_production_authorization']
        self.assertTrue(self.validate(value))

    def test_missing_result_fails_even_on_zero_exit(self):
        self.assertTrue(self.validate(None))

    def run_main(self, core):
        values=[(0, core, '')]
        schemas=['cex.external-agent-runtime-boundary-check.v1',
                 'cex.execution-default-state-boundary-check.v1',
                 'cex.external-production-evidence-contract-check.v1',
                 'cex.external-production-evidence-binding-self-test.v1']
        values.extend((0, child(schema=s), '') for s in schemas)
        output=io.StringIO()
        with patch.object(gate,'run_json',side_effect=values), contextlib.redirect_stdout(output):
            code=gate.main()
        return code,json.loads(output.getvalue())

    def test_failed_core_cannot_be_rewritten_to_ok(self):
        value=gate.fallback_result(''); value['problems']=[]
        code,output=self.run_main(value)
        self.assertEqual(code,1); self.assertEqual(output['status'],'failed')

    def test_core_authority_is_validated_before_normalization(self):
        for changes in [dict(production_authorization='granted'), dict(requirements=True),
                        dict(schema='wrong'),dict(repository_qualification_result='CLOSED')]:
            value=gate.fallback_result(''); value.update(status='ok',problems=[]); value.update(changes)
            code,output=self.run_main(value)
            self.assertEqual(code,1)
            self.assertEqual(output['production_authorization'],'not_granted')

    def test_valid_core_and_children_remain_successful(self):
        value=gate.fallback_result(''); value.update(status='ok',problems=[])
        code,output=self.run_main(value)
        self.assertEqual(code,0); self.assertEqual(output['status'],'ok')


class ChildTransportTests(unittest.TestCase):
    def run_child(self, source):
        return gate.run_json([sys.executable,'-c',source], 'fixture child')

    def test_stdout_result_and_stderr_diagnostics_are_separate(self):
        code,value,_=self.run_child('import sys; print("diagnostic",file=sys.stderr); print("{}")')
        self.assertEqual(code,0); self.assertEqual(value,{})

    def test_stderr_only_json_is_not_a_result(self):
        _,value,_=self.run_child('import sys; print("{}",file=sys.stderr)')
        self.assertIsNone(value)

    def test_duplicate_json_members_are_rejected(self):
        for text in ['{"status":"failed","status":"ok"}', '{"nested":{"a":1,"a":2}}']:
            _,value,_=self.run_child(f'print({text!r})')
            self.assertIsNone(value)

    def test_nonfinite_values_and_nonobject_roots_are_rejected(self):
        for text in ['{"x":NaN}', '{"x":Infinity}', '[]', 'null', 'true', '{} {}']:
            _,value,_=self.run_child(f'print({text!r})')
            self.assertIsNone(value)

    def test_invalid_bytes_and_private_output_are_not_echoed(self):
        for source in ['print("PRIVATE-RESEARCH-MARKER")', 'import sys; sys.stdout.buffer.write(bytes([255]))']:
            _,value,diagnostic=self.run_child(source)
            self.assertIsNone(value); self.assertNotIn('PRIVATE-RESEARCH-MARKER',diagnostic)

    def test_timeout_is_a_failure(self):
        with patch.object(gate,'CHECK_TIMEOUT_SECONDS',0.15):
            code,value,diagnostic=self.run_child('import time; time.sleep(5)')
        self.assertNotEqual(code,0); self.assertIsNone(value); self.assertIn('timeout',diagnostic)

    def test_stdout_and_stderr_limits_are_enforced(self):
        for stream in ['stdout','stderr']:
            with patch.object(gate,'MAX_OUTPUT_BYTES',1024):
                code,value,diagnostic=self.run_child(f'import sys; sys.{stream}.write("x" * 65536)')
            self.assertNotEqual(code,0); self.assertIsNone(value); self.assertIn('limit',diagnostic)

    def test_missing_executable_is_reported_without_traceback(self):
        code,value,_=gate.run_json([str(ROOT/'definitely-missing-executable')], 'missing fixture')
        self.assertNotEqual(code,0); self.assertIsNone(value)


class ModuleSectionTests(unittest.TestCase):
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory(); self.root=Path(self.temp.name)
        self.override=patch.object(docs,'ROOT',self.root); self.override.start()
        self.addCleanup(self.override.stop); self.addCleanup(self.temp.cleanup)
        docs.PROBLEMS.clear()
        self.path=self.root/'docs/modules/example.md'; self.path.parent.mkdir(parents=True)

    def document(self):
        text='Workspace member: `crates/example`\nPackage: `example`\nOwner role: `owner`\nProduction authorization: `not_granted`\n'
        for marker in docs.REQUIRED_SECTIONS:
            text+='\n'+marker+'\n'+('This is a bounded module contract with explicit ownership, deterministic validation and recovery. ' * 2)+'\n'
            if marker=='## Deployment and operations':
                text+='Not independently deployable.\n'
        return text

    def check(self,text):
        self.path.write_text(text,encoding='utf-8'); docs.PROBLEMS.clear()
        docs.validate_document(member='crates/example',package='example',kind='library',
                               deployable=False,document='docs/modules/example.md',owner='owner',
                               entrypoints=[],commands=[],index_text='crates/example example [contract](example.md)')
        return list(docs.PROBLEMS)

    def test_valid_document_passes(self):
        self.assertEqual(self.check(self.document()),[])

    def test_empty_required_section_is_rejected(self):
        text=self.document(); a=text.index('## Security and trust boundaries'); b=text.index('## Verification')
        self.assertTrue(self.check(text[:a]+'## Security and trust boundaries\n\n'+text[b:]))

    def test_missing_and_duplicate_headings_are_rejected(self):
        text=self.document()
        self.assertTrue(self.check(text.replace('## Security and trust boundaries','## Other')))
        self.assertTrue(self.check(text+'\n## Security and trust boundaries\n'+'x'*100))

    def test_comment_only_content_is_rejected(self):
        text=self.document(); a=text.index('## Security and trust boundaries'); b=text.index('## Verification')
        self.assertTrue(self.check(text[:a]+'## Security and trust boundaries\n<!--'+'x'*150+'-->\n'+text[b:]))

    def test_headings_inside_fences_and_comments_are_not_real(self):
        for prefix,suffix in [('```markdown\n','\n```'),('~~~\n','\n~~~'),('<!--\n','\n-->')]:
            text=self.document(); marker='## Security and trust boundaries'
            self.assertTrue(self.check(text.replace(marker,prefix+marker+suffix)))

    def test_fenced_examples_remain_section_content(self):
        text='## Verification\n```sh\ncargo test -p example\n## not a heading\n```\n## Next\nbody'
        body=docs.section_body(text,'## Verification')
        self.assertIn('cargo test',body); self.assertIn('## not a heading',body)
        self.assertNotIn('## Next',body)

    def test_exact_heading_match_and_crlf(self):
        text='## Verification extra\r\nother\r\n## Verification\r\nexpected\r\n'
        self.assertEqual(docs.section_body(text,'## Verification'),'expected')


def run_blocks(text):
    blocks=[]; current=None
    for line in text.splitlines():
        if line.strip()=='run: |':
            if current is not None: blocks.append('\n'.join(current))
            current=[]
        elif current is not None:
            if line.startswith('          '): current.append(line[10:])
            elif not line.strip(): current.append('')
            else: blocks.append('\n'.join(current)); current=None
    if current is not None: blocks.append('\n'.join(current))
    return blocks


class DeploymentInputTests(unittest.TestCase):
    def workflows(self):
        for name in ['self-hosted-desktop-availability.yml','self-hosted-fleet-availability.yml']:
            yield (ROOT/'.github/workflows'/name).read_text(encoding='utf-8')

    def test_probe_inputs_are_never_interpolated_into_shell_code(self):
        count=0
        for text in self.workflows():
            self.assertIn('PROBE_REASON: ${{ inputs.reason }}',text)
            self.assertIn('permissions: {}',text)
            self.assertNotIn('actions/checkout@',text)
            self.assertIn("github.ref == 'refs/heads/main'",text)
            for block in run_blocks(text):
                self.assertNotIn('${{',block)
                proc=subprocess.run(['bash','-n'],input=block,text=True,capture_output=True,check=False)
                self.assertEqual(proc.returncode,0,proc.stderr)
                if 'PROBE_REASON' in block: count+=1
        self.assertEqual(count,4)

    def test_probe_reason_is_data_not_executable_code(self):
        with tempfile.TemporaryDirectory() as tmp:
            sentinel=Path(tmp)/'injection-sentinel'
            reason=f"'; touch {sentinel}; # $(touch {sentinel})\nUNTRUSTED"
            for text in self.workflows():
                for block in run_blocks(text):
                    if 'PROBE_REASON' not in block: continue
                    runner=re.search(r'test "\$RUNNER_NAME" = ([a-z0-9-]+)',block).group(1)
                    env={'PATH':os.environ.get('PATH',''),'GITHUB_EVENT_NAME':'workflow_dispatch',
                         'GITHUB_REF':'refs/heads/main','RUNNER_NAME':runner,'RUNNER_OS':'Linux',
                         'RUNNER_ARCH':'X64','PROBE_REASON':reason}
                    proc=subprocess.run(['bash','-c',block],env=env,text=True,capture_output=True,check=False)
                    self.assertEqual(proc.returncode,0,proc.stderr)
                    self.assertFalse(sentinel.exists())

    def test_probe_rejects_nonmain_ref_before_diagnostics(self):
        for text in self.workflows():
            for block in run_blocks(text):
                if 'PROBE_REASON' not in block: continue
                env={'PATH':os.environ.get('PATH',''),'GITHUB_EVENT_NAME':'workflow_dispatch',
                     'GITHUB_REF':'refs/heads/untrusted','PROBE_REASON':'fixture'}
                proc=subprocess.run(['bash','-c',block],env=env,text=True,capture_output=True,check=False)
                self.assertNotEqual(proc.returncode,0)
                self.assertNotIn('reason=',proc.stdout)

    def test_example_shell_preserves_json_and_does_not_opt_in(self):
        path=ROOT/'.env.external-agent.example'
        text=path.read_text(encoding='utf-8')
        self.assertNotIn('CEX_ENABLE_LEGACY_LOCAL_PROVIDER_DISPATCH=true',text)
        program='import os,json; v=json.loads(os.environ["CAPABILITY_EXTERNAL_AGENT_REGISTRY_JSON"]); assert v[0]["kind"]=="external_agent_capability"; assert os.environ.get("CEX_ENABLE_LEGACY_LOCAL_PROVIDER_DISPATCH") is None'
        proc=subprocess.run(['bash','-c','set -eu; set -a; source "$1"; "$2" -c "$3"',
                             'fixture',str(path),sys.executable,program],
                            env={'PATH':os.environ.get('PATH','')},text=True,capture_output=True,check=False)
        self.assertEqual(proc.returncode,0,proc.stderr)


class CatalogRepairTests(unittest.TestCase):
    def repaired(self):
        names={'ledger-service','trnm-economy-service','gateway-service','execution-service','audit-service','paper-raid-bff'}
        catalog=json.loads((ROOT/'docs/module-catalog-v1.json').read_text(encoding='utf-8'))
        return [m for m in catalog['modules'] if m['package'] in names]

    def test_six_library_entrypoints_are_catalogued_once(self):
        modules=self.repaired(); self.assertEqual(len(modules),6)
        for module in modules:
            self.assertEqual(module['source_entrypoints'].count(module['workspace_member']+'/src/lib.rs'),1)

    def test_repaired_module_contracts_pass_content_and_entrypoint_checks(self):
        for module in self.repaired():
            docs.PROBLEMS.clear()
            docs.validate_document(member=module['workspace_member'],package=module['package'],
                kind=module['kind'],deployable=module['deployable'],document=module['documentation'],
                owner=module['owner'],entrypoints=module['source_entrypoints'],commands=module['verification'],
                index_text=module['workspace_member']+' '+module['package']+' [contract]('+Path(module['documentation']).name+')')
            self.assertEqual(docs.PROBLEMS,[],module['package'])


if __name__ == '__main__':
    unittest.main(verbosity=2)
