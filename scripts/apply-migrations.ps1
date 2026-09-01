[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot '_dev-helpers.ps1')
Import-CexDotEnv
$dockerExe = Get-CexDockerExe
Wait-CexPostgresReady

$migrations = Get-ChildItem -LiteralPath (Join-Path (Split-Path -Parent $PSScriptRoot) 'migrations') -File -Filter *.sql | Sort-Object Name
foreach ($migration in $migrations) {
    Write-Host "Applying migration $($migration.Name)..."
    $sql = Get-Content -LiteralPath $migration.FullName -Raw
    $sql | & $dockerExe exec -i cex-postgres-1 psql -U postgres -d cex_ai -v ON_ERROR_STOP=1 -f -
    if ($LASTEXITCODE -ne 0) {
        throw "Migration failed: $($migration.Name)"
    }
}

Write-Host 'All migrations applied successfully.'

