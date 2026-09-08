#!/usr/bin/env python3
"""Verify live CEX main ruleset enforcement and reject source-only substitutes."""
from __future__ import annotations
import json, os, sys, urllib.request
TOKEN=os.environ.get('GITHUB_TOKEN') or os.environ.get('GH_TOKEN')
REPO=os.environ.get('GITHUB_REPOSITORY','TrillionniumFoundation/CEX')
if not TOKEN: raise SystemExit('GITHUB_TOKEN or GH_TOKEN is required')
headers={'Accept':'application/vnd.github+json','Authorization':f'Bearer {TOKEN}','X-GitHub-Api-Version':'2022-11-28','User-Agent':'cex-ruleset-verifier'}
def get(path):
 req=urllib.request.Request(f'https://api.github.com/repos/{REPO}/{path}',headers=headers)
 with urllib.request.urlopen(req,timeout=30) as r: return json.load(r)
branch=get('branches/main'); rulesets=get('rulesets?per_page=100')
active=[r for r in rulesets if r.get('enforcement')=='active' and r.get('target')=='branch']
problems=[]
if not branch.get('protected'): problems.append('main is not protected')
if not active: problems.append('no active branch ruleset')
result={'schema':'cex.repository-ruleset-readback.v1','status':'failed' if problems else 'ok','main_protected':bool(branch.get('protected')),'active_branch_rulesets':active,'production_authorization':'not_granted','problems':problems}
print(json.dumps(result,indent=2,sort_keys=True)); sys.exit(1 if problems else 0)
