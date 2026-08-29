[CmdletBinding()]
param(
    [string]$OrgId = '00000000-0000-0000-0000-00000000ce01',
    [string]$OrgName = 'Local Dev Org',
    [string]$UserId = '00000000-0000-0000-0000-00000000ce11',
    [string]$ApiKeyId = '00000000-0000-0000-0000-00000000ce21',
    [string]$ApiKey = 'local-dev-key'
)

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot '_dev-helpers.ps1')
Import-CexDotEnv
$dockerExe = Get-CexDockerExe
Wait-CexPostgresReady

$sql = @"
insert into organizations (org_id, name, status, plan)
values ('$OrgId', '$OrgName', 'active', 'dev')
on conflict (org_id) do update set name = excluded.name, status = excluded.status, plan = excluded.plan, updated_at = now();

insert into users (user_id, org_id, email, role, status)
values ('$UserId', '$OrgId', 'local-dev@example.test', 'owner', 'active')
on conflict (user_id) do update set org_id = excluded.org_id, email = excluded.email, role = excluded.role, status = excluded.status, updated_at = now();

insert into api_keys (api_key_id, org_id, user_id, key_hash, key_prefix, label, status, revoked_at, expires_at)
values (
    '$ApiKeyId',
    '$OrgId',
    '$UserId',
    encode(digest('$ApiKey', 'sha256'), 'hex'),
    'local-dev',
    'Local Dev Key',
    'active',
    null,
    null
)
on conflict (key_hash) do update
set org_id = excluded.org_id,
    user_id = excluded.user_id,
    key_prefix = excluded.key_prefix,
    label = excluded.label,
    status = excluded.status,
    revoked_at = null,
    expires_at = null;
"@

$sql | & $dockerExe exec -i cex-postgres-1 psql -U postgres -d cex_ai -v ON_ERROR_STOP=1 -f -
if ($LASTEXITCODE -ne 0) {
    throw 'Failed to seed local dev org/user/api key.'
}

Write-Host "Seeded local dev org, user, and API key provenance: $OrgId / $UserId / $ApiKeyId"
