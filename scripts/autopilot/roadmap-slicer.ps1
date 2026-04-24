[CmdletBinding()]
param(
    [string]$ProjectRoot,
    [string]$JobId = '',
    [Parameter(Mandatory = $true)][string]$Domain
)
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
. (Join-Path $PSScriptRoot 'worker-common.ps1')
Initialize-AutopilotRuntime -ProjectRoot $ProjectRoot | Out-Null

switch ($Domain) {
    'identity' {
        $tasks = @(
            (New-QueueTask -Domain $Domain -Id 'identity-001-schema' -Title 'Define identity schema' -Goal 'Add durable schema support for orgs, users, and API keys.' -Acceptance @('migration added','identity service can persist org/user/api-key records') -TargetPaths @('migrations/','services/identity-service/')),
            (New-QueueTask -Domain $Domain -Id 'identity-002-api' -Title 'Implement identity CRUD APIs' -Goal 'Expose org/user/api-key management endpoints beyond /health.' -DependsOn @('identity-001-schema') -Acceptance @('HTTP routes exist','basic persistence-backed tests added') -TargetPaths @('services/identity-service/')),
            (New-QueueTask -Domain $Domain -Id 'identity-003-authn' -Title 'Add authn/authz primitives' -Goal 'Introduce API-key verification and actor/org context resolution.' -DependsOn @('identity-001-schema','identity-002-api') -Acceptance @('API key validation path exists','shared actor context shape is defined') -TargetPaths @('services/identity-service/','crates/shared-types/','crates/shared-config/')),
            (New-QueueTask -Domain $Domain -Id 'identity-004-gateway-integration' -Title 'Integrate gateway auth' -Goal 'Make gateway require and propagate authenticated org/actor context.' -DependsOn @('identity-003-authn') -Acceptance @('gateway auth middleware present','invocation provenance includes org/actor context') -TargetPaths @('services/gateway-service/','services/identity-service/'))
        )
    }
    'capability' {
        $tasks = @(
            (New-QueueTask -Domain $Domain -Id 'capability-001-model' -Title 'Define capability registry model' -Goal 'Turn capability_id from a loose field into a durable registry model.' -Acceptance @('registry schema defined','capability metadata contract documented') -TargetPaths @('migrations/','crates/shared-types/')),
            (New-QueueTask -Domain $Domain -Id 'capability-002-api' -Title 'Add capability registry API' -Goal 'Expose CRUD/query endpoints for capabilities and versions.' -DependsOn @('capability-001-model') -Acceptance @('list/get/create endpoints exist','version metadata supported') -TargetPaths @('services/','crates/shared-types/')),
            (New-QueueTask -Domain $Domain -Id 'capability-003-gateway-validation' -Title 'Validate capability references at ingress' -Goal 'Have gateway reject unknown or disabled capabilities.' -DependsOn @('capability-002-api') -Acceptance @('gateway lookup path exists','invocation rejects invalid capability ids') -TargetPaths @('services/gateway-service/','services/')),
            (New-QueueTask -Domain $Domain -Id 'capability-004-provider-mapping' -Title 'Attach provider mapping metadata' -Goal 'Store provider/runtime compatibility and tags per capability/version.' -DependsOn @('capability-002-api') -Acceptance @('provider mapping metadata exists','selection layer can read it') -TargetPaths @('services/','crates/shared-types/'))
        )
    }
    'policy' {
        $tasks = @(
            (New-QueueTask -Domain $Domain -Id 'policy-001-service-skeleton' -Title 'Create policy-risk service skeleton' -Goal 'Introduce a dedicated policy-risk service boundary.' -Acceptance @('service exists in workspace','health/evaluate contract stub exists') -TargetPaths @('services/','crates/shared-types/')),
            (New-QueueTask -Domain $Domain -Id 'policy-002-evaluate-contract' -Title 'Define policy decision contract' -Goal 'Create a durable request/response schema for policy evaluation.' -DependsOn @('policy-001-service-skeleton') -Acceptance @('shared policy types exist','decision includes approval_required and reason') -TargetPaths @('crates/shared-types/','services/')),
            (New-QueueTask -Domain $Domain -Id 'policy-003-execution-extraction' -Title 'Extract execution policy heuristics' -Goal 'Move in-process policy checks out of execution-service behind the new service boundary.' -DependsOn @('policy-002-evaluate-contract') -Acceptance @('execution calls policy service','fallback behavior is explicit') -TargetPaths @('services/execution-service/','services/')),
            (New-QueueTask -Domain $Domain -Id 'policy-004-approval-explainability' -Title 'Improve approval reasoning' -Goal 'Persist policy explanations and approval routing rationale.' -DependsOn @('policy-003-execution-extraction') -Acceptance @('decision explanation persisted','approval reason visible in runtime records') -TargetPaths @('services/execution-service/','migrations/'))
        )
    }
    'provider' {
        $tasks = @(
            (New-QueueTask -Domain $Domain -Id 'provider-001-adapter-trait' -Title 'Define provider adapter trait' -Goal 'Create execution/provider adapter boundaries.' -Acceptance @('adapter trait exists','selection inputs are defined') -TargetPaths @('services/execution-service/','crates/shared-types/')),
            (New-QueueTask -Domain $Domain -Id 'provider-002-first-adapter' -Title 'Implement first provider adapter' -Goal 'Add one real provider-backed execution path.' -DependsOn @('provider-001-adapter-trait') -Acceptance @('first adapter implemented','execution can hand off to provider path') -TargetPaths @('services/execution-service/')),
            (New-QueueTask -Domain $Domain -Id 'provider-003-runtime-persistence' -Title 'Persist provider runtime outputs' -Goal 'Store provider request/response metadata for replay and audit.' -DependsOn @('provider-002-first-adapter') -Acceptance @('provider run records persist','audit trail enriched') -TargetPaths @('migrations/','services/execution-service/','services/audit-service/')),
            (New-QueueTask -Domain $Domain -Id 'provider-004-blackbox-tests' -Title 'Add provider blackbox coverage' -Goal 'Cover provider-backed execution in runtime tests.' -DependsOn @('provider-002-first-adapter') -Acceptance @('runtime blackbox includes provider flow','CI can gate provider path safely') -TargetPaths @('services/gateway-service/tests/','scripts/'))
        )
    }
    'async' {
        $tasks = @(
            (New-QueueTask -Domain $Domain -Id 'async-001-event-shapes' -Title 'Define runtime event shapes' -Goal 'Create durable event contracts for invocation/execution lifecycle changes.' -Acceptance @('event payload types exist','publish points identified') -TargetPaths @('crates/shared-types/','services/')),
            (New-QueueTask -Domain $Domain -Id 'async-002-first-publishers' -Title 'Publish first runtime events' -Goal 'Emit lifecycle events from gateway/execution services.' -DependsOn @('async-001-event-shapes') -Acceptance @('publisher code exists','events emitted on key transitions') -TargetPaths @('services/gateway-service/','services/execution-service/')),
            (New-QueueTask -Domain $Domain -Id 'async-003-first-consumers' -Title 'Add consumer skeletons' -Goal 'Create first NATS/Redis consumers for downstream handling.' -DependsOn @('async-002-first-publishers') -Acceptance @('consumer worker exists','basic end-to-end event flow works locally') -TargetPaths @('services/','docker-compose.yml')),
            (New-QueueTask -Domain $Domain -Id 'async-004-retry-policy' -Title 'Define retry/dead-letter behavior' -Goal 'Add failure handling semantics for async delivery.' -DependsOn @('async-003-first-consumers') -Acceptance @('retry policy documented','dead-letter or error sink defined') -TargetPaths @('docs/','services/'))
        )
    }
    'refactor' {
        $tasks = @(
            (New-QueueTask -Domain $Domain -Id 'refactor-001-execution-split' -Title 'Split execution-service API file' -Goal 'Move oversized api.rs responsibilities into focused modules.' -Acceptance @('handlers split out','state/persistence helpers separated') -TargetPaths @('services/execution-service/')),
            (New-QueueTask -Domain $Domain -Id 'refactor-002-gateway-split' -Title 'Split gateway orchestration concerns' -Goal 'Separate ingress, orchestration, persistence, and upstream client logic.' -DependsOn @('refactor-001-execution-split') -Acceptance @('gateway service layers are clearer','upstream clients isolated') -TargetPaths @('services/gateway-service/')),
            (New-QueueTask -Domain $Domain -Id 'refactor-003-shared-contract-cleanup' -Title 'Clean shared contracts' -Goal 'Clarify shared crate boundaries while platform services multiply.' -Acceptance @('shared-types/config/errors scopes are clearer','cross-service coupling reduced') -TargetPaths @('crates/')),
            (New-QueueTask -Domain $Domain -Id 'refactor-004-doc-sync' -Title 'Keep docs aligned with refactors' -Goal 'Prevent architecture/docs drift during service decomposition.' -Acceptance @('docs updated with new boundaries','CI/gate docs still accurate') -TargetPaths @('docs/','ops/autopilot/'))
        )
    }
    default {
        throw ('Unsupported Domain=' + $Domain)
    }
}

Write-QueueBundle -ProjectRoot $ProjectRoot -Domain $Domain -Tasks $tasks
Write-Host ('roadmap-slicer: domain=' + $Domain + ' tasks=' + @($tasks).Count)
