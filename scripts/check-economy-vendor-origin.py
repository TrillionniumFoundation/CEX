#!/usr/bin/env python3
"""Verify the complete CEX source lineage of the vendored economy protocol.

The historical external path is context only. Repository provenance begins at the
exact import bytes accepted by ADR-005. This checker does not grant production.
"""
from __future__ import annotations
import argparse, json, subprocess
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
RECORD = ROOT / 'vendor/trnm-economy-vendor-origin-v1.json'
SCHEMA = 'cex.trnm-economy-vendor-origin-check.v2'
EXPECTED = [
    ('ce681c60d7e09a25f468815a6e7ae6081e8e1a12','93dcbc7d7dc9620b9558895f0639a63e67cc0919','a5128176ed31661c7044a54bb8e8f6257287cb4b','92e602b3601fb3935b63ae99cbad1351ac228dc6'),
    ('836bddf1c1efa6d90aa7893256df2b88103b9d53','91c39dbc4d5ada39d272f32b7cc943d49888bfbf','2a1a1a3ef81d2a5be30a73cdf18d9a6384f6eb33','9d075c11a392da873f0a84cd1e0aea5533ab4eaa'),
    ('ba399db7de9f80062414427a5b8207f37d91229a','8101ca1e70a534d11c8161a2e69deec23ccc0ad2','41d1917045733005725148b2c4a98fa890353d0e','8c88d4708a273ba99da71a91105ea7ef1bf872dd'),
    ('81b49fc27ece5c3a35c9eba6023dc1f65a8b6a1f','4bf9e5de88cbe54d90802d18cbdfa8a21f19ded3','4c4751d3a8a23d3b9c65c4bf11141aab77977ac4','6d6def11a1cb06540aebe9400701af8d0c91c912'),
]


def unique_object(pairs):
    out = {}
    for key, value in pairs:
        if key in out: raise ValueError('duplicate_json_field')
        out[key] = value
    return out


def git_identity(revision: str, path: str) -> str:
    process = subprocess.run(['git','-C',str(ROOT),'rev-parse',f'{revision}:{path}'],
                             stdin=subprocess.DEVNULL,capture_output=True,text=True,
                             timeout=10,check=False)
    if process.returncode != 0: raise ValueError('git_identity_unavailable')
    value=process.stdout.strip()
    if len(value)!=40 or any(c not in '0123456789abcdef' for c in value): raise ValueError('invalid_git_identity')
    return value


def validate(record: dict[str, Any], identities: dict[tuple[str,str],str]) -> list[str]:
    p=[]
    if record.get('schema')!='cex.trnm-economy-vendor-origin.v2': p.append('record_schema')
    if record.get('production_authorization')!='not_granted': p.append('authority_overclaim')
    if record.get('package')!='trnm-economy-protocol' or record.get('package_version')!='2.4.0': p.append('package_identity')
    if record.get('current_path')!='vendor/trnm-economy-protocol': p.append('current_path')
    genesis=record.get('source_genesis')
    lineage=record.get('cex_lineage')
    if not isinstance(genesis,dict) or not isinstance(lineage,list) or len(lineage)!=3:
        return p+['lineage_shape']
    rows=[genesis,*lineage]
    for row, expected in zip(rows, EXPECTED):
        commit, tree, cargo, lib = expected
        if row.get('commit')!=commit or row.get('git_tree')!=tree: p.append('lineage_identity:'+commit)
        files=row.get('files')
        if not isinstance(files,dict) or files.get('Cargo.toml')!=cargo or files.get('src/lib.rs')!=lib:
            p.append('lineage_file_identity:'+commit)
        for path, expected_id in [('vendor/trnm-economy-protocol',tree),('vendor/trnm-economy-protocol/Cargo.toml',cargo),('vendor/trnm-economy-protocol/src/lib.rs',lib)]:
            if identities.get((commit,path))!=expected_id: p.append('git_history_drift:'+commit+':'+path)
    if genesis.get('authority')!='CEX_import_commit' or genesis.get('decision')!='decisions/adr-005-trnm-economy-source-genesis.md': p.append('genesis_authority')
    historical=record.get('historical_external_context')
    if not isinstance(historical,dict) or historical.get('status')!='unknown_non_authoritative_prehistory' or historical.get('immutable_repository_commit_tree_recorded_at_import') is not False:
        p.append('historical_context_overclaim')
    if historical.get('previous_path')!='../Trillionnium/trillionnium/crates/trnm-economy-protocol': p.append('historical_path')
    current=record.get('current_files')
    last=EXPECTED[-1]
    if record.get('current_git_tree')!=last[1] or not isinstance(current,dict) or current.get('Cargo.toml')!=last[2] or current.get('src/lib.rs')!=last[3]: p.append('current_record_identity')
    for path, expected_id in [('vendor/trnm-economy-protocol',last[1]),('vendor/trnm-economy-protocol/Cargo.toml',last[2]),('vendor/trnm-economy-protocol/src/lib.rs',last[3])]:
        if identities.get(('HEAD',path))!=expected_id: p.append('current_tree_drift:'+path)
    policy=record.get('future_update_policy')
    if not isinstance(policy,str) or not policy.strip(): p.append('future_update_policy')
    return p


def main() -> int:
    parser=argparse.ArgumentParser(); parser.add_argument('--contract-only',action='store_true'); parser.parse_args()
    result={'schema':SCHEMA,'status':'failed','provenance_resolved':False,
            'checker_may_grant_production_authorization':False,
            'production_authorization':'not_granted','problems':[]}
    try:
        record=json.loads(RECORD.read_text('utf-8'),object_pairs_hook=unique_object)
        identities={}
        for commit,_,_,_ in EXPECTED:
            for path in ('vendor/trnm-economy-protocol','vendor/trnm-economy-protocol/Cargo.toml','vendor/trnm-economy-protocol/src/lib.rs'):
                identities[(commit,path)]=git_identity(commit,path)
        for path in ('vendor/trnm-economy-protocol','vendor/trnm-economy-protocol/Cargo.toml','vendor/trnm-economy-protocol/src/lib.rs'):
            identities[('HEAD',path)]=git_identity('HEAD',path)
        result['problems']=validate(record,identities)
        result['provenance_resolved']=not result['problems']
        result['status']='ok' if not result['problems'] else 'failed'
    except (OSError,ValueError,TypeError,json.JSONDecodeError,subprocess.TimeoutExpired):
        result['problems']=['origin_record_or_git_identity_invalid']
    print(json.dumps(result,sort_keys=True,indent=2))
    return 0 if result['status']=='ok' else 1

if __name__=='__main__': raise SystemExit(main())
