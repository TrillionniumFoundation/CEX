#!/usr/bin/env bash
# Shared constants plus text/JSON surface helpers for the operator-signal cron install CLIs.

OSC_OPERATOR_SIGNAL_DEFAULT_POLICY_PROFILE="default"
OSC_OPERATOR_SIGNAL_RUN_COMMAND_BASE="./scripts/run-operator-signal-check.sh --compact"
OSC_OPERATOR_SIGNAL_SUPPORTED_POLICY_BUNDLES=("entry-identity" "monitoring-deploy" "baseline")
OSC_OPERATOR_SIGNAL_SUPPORTED_POLICY_PROFILES=("default" "identity" "deploy")

OSC_INSTALL_SUMMARY="Repo-root convenience entrypoint for the operator-signal OpenClaw cron installer."
OSC_INSTALL_USAGE="./install-operator-signal-cron.sh [--dry-run] [--status] [--status-json] [--examples] [--doctor] [--doctor-json] [--help-json] [--schema] [extra helper flags...]"
OSC_INSTALL_DEFAULT_ACTION="install"
OSC_INSTALL_DEFAULT_POLICY_PROFILE="$OSC_OPERATOR_SIGNAL_DEFAULT_POLICY_PROFILE"

OSC_SCRIPT_INSTALL_SUMMARY="Thin one-liner wrapper around scripts/register-openclaw-operator-signal-cron.sh."
OSC_SCRIPT_INSTALL_USAGE="scripts/install-operator-signal-cron.sh [--dry-run] [--show] [--remove] [--run-now] [--print-run-command] [--print-message] [extra helper flags...]"
OSC_REGISTER_SUMMARY="Linux/bash helper around the OpenClaw cron CLI for the repo-local operator-signal monitor."
OSC_REGISTER_USAGE="scripts/register-openclaw-operator-signal-cron.sh [--action install|show|remove|run-now] [--recreate] [--dry-run] [--print-run-command] [--print-message] [--use-entry-identity-policy-example] [--use-monitoring-deploy-policy-example] [--policy-bundle <name>] [--policy-profile <name>]"
OSC_POWERSHELL_REGISTER_SUMMARY="PowerShell helper around the OpenClaw cron CLI for the repo-local operator-signal monitor."
OSC_POWERSHELL_REGISTER_USAGE="powershell -ExecutionPolicy Bypass -File .\\scripts\\register-openclaw-operator-signal-cron.ps1 [-Action install|show|remove|run-now] [-Recreate] [-UseEntryIdentityPolicyExample] [-UseMonitoringDeployPolicyExample] [-PolicyBundle <name>] [-PolicyProfile <name>]"

OSC_INSTALL_HELP_KIND="operator-signal-cron-install-help"
OSC_INSTALL_HELP_SCHEMA_VERSION=1
OSC_INSTALL_STATUS_KIND="operator-signal-cron-install-status"
OSC_INSTALL_STATUS_SCHEMA_VERSION=1
OSC_INSTALL_DOCTOR_KIND="operator-signal-cron-install-doctor"
OSC_INSTALL_DOCTOR_SCHEMA_VERSION=1
OSC_INSTALL_SCHEMA_KIND="operator-signal-cron-install-schema-catalog"
OSC_INSTALL_SCHEMA_VERSION=1
OSC_OPERATOR_SIGNAL_CATALOG_KIND="operator-signal-cron-cli-catalog"
OSC_OPERATOR_SIGNAL_CATALOG_SCHEMA_VERSION=1
OSC_OPERATOR_SIGNAL_CATALOG_READER_SUMMARY="Read the shared machine-readable catalog for the operator-signal cron install CLIs."
OSC_OPERATOR_SIGNAL_CATALOG_READER_USAGE="./scripts/read-operator-signal-cron-catalog.sh [--compact] [--field <dotted.path>] [--help-json] [--schema]"
OSC_OPERATOR_SIGNAL_CATALOG_READER_HELP_KIND="operator-signal-cron-cli-catalog-reader-help"
OSC_OPERATOR_SIGNAL_CATALOG_READER_HELP_SCHEMA_VERSION=1
OSC_OPERATOR_SIGNAL_CATALOG_READER_SCHEMA_KIND="operator-signal-cron-cli-catalog-reader-schema"
OSC_OPERATOR_SIGNAL_CATALOG_READER_SCHEMA_VERSION=1

osc_operator_signal_has_explicit_policy_flags() {
  local args=("$@")
  local index=0
  while [[ $index -lt ${#args[@]} ]]; do
    case "${args[$index]}" in
      --policy-profile|--policy-bundle|--use-entry-identity-policy-example|--use-monitoring-deploy-policy-example)
        return 0
        ;;
    esac
    index=$((index + 1))
  done
  return 1
}

osc_operator_signal_validate_unique_list() {
  local array_name="$1"
  declare -n arr="$array_name"
  local -A seen=()
  local deduped=()
  local item
  for item in "${arr[@]}"; do
    [[ -n "$item" ]] || continue
    if [[ -z "${seen[$item]:-}" ]]; then
      seen[$item]=1
      deduped+=("$item")
    fi
  done
  arr=("${deduped[@]}")
}

osc_operator_signal_join_by_comma_space() {
  local out=""
  local item
  for item in "$@"; do
    [[ -z "$item" ]] && continue
    if [[ -n "$out" ]]; then
      out+=", "
    fi
    out+="$item"
  done
  printf '%s\n' "$out"
}

osc_operator_signal_array_contains() {
  local needle="$1"
  shift
  local item
  for item in "$@"; do
    [[ "$item" == "$needle" ]] && return 0
  done
  return 1
}

osc_operator_signal_validate_policy_bundle() {
  local value="$1"
  osc_operator_signal_array_contains "$value" "${OSC_OPERATOR_SIGNAL_SUPPORTED_POLICY_BUNDLES[@]}"
}

osc_operator_signal_validate_policy_profile() {
  local value="$1"
  osc_operator_signal_array_contains "$value" "${OSC_OPERATOR_SIGNAL_SUPPORTED_POLICY_PROFILES[@]}"
}

osc_operator_signal_supported_policy_bundles_display() {
  osc_operator_signal_join_by_comma_space "${OSC_OPERATOR_SIGNAL_SUPPORTED_POLICY_BUNDLES[@]}"
}

osc_operator_signal_supported_policy_bundles_json() {
  printf '%s\n' "${OSC_OPERATOR_SIGNAL_SUPPORTED_POLICY_BUNDLES[@]}" | jq -R . | jq -s .
}

osc_operator_signal_supported_policy_profiles_display() {
  osc_operator_signal_join_by_comma_space "${OSC_OPERATOR_SIGNAL_SUPPORTED_POLICY_PROFILES[@]}"
}

osc_operator_signal_supported_policy_profiles_json() {
  printf '%s\n' "${OSC_OPERATOR_SIGNAL_SUPPORTED_POLICY_PROFILES[@]}" | jq -R . | jq -s .
}

osc_script_install_examples_json() {
  jq -cn '[
    "./scripts/install-operator-signal-cron.sh",
    "./scripts/install-operator-signal-cron.sh --dry-run",
    "./scripts/install-operator-signal-cron.sh --print-run-command",
    "./scripts/install-operator-signal-cron.sh --print-message",
    "./scripts/install-operator-signal-cron.sh --show",
    "./scripts/install-operator-signal-cron.sh --policy-profile deploy"
  ]'
}

osc_script_install_print_usage() {
  cat <<EOF
Usage: ${OSC_SCRIPT_INSTALL_USAGE}

${OSC_SCRIPT_INSTALL_SUMMARY}

Defaults:
  - action=install
  - policy-profile=${OSC_OPERATOR_SIGNAL_DEFAULT_POLICY_PROFILE} (only when no explicit policy flags are provided)

Shortcuts:
  --dry-run            Pass through to the bash helper dry-run
  --show               Equivalent to --action show
  --remove             Equivalent to --action remove
  --run-now            Equivalent to --action run-now
  --print-run-command  Print only the generated repo-local runCommand
  --print-message      Print only the generated OpenClaw cron message body

Examples:
$(osc_script_install_examples_json | jq -r '.[] | "  " + .')
EOF
}

osc_register_examples_json() {
  jq -cn '[
    "./scripts/register-openclaw-operator-signal-cron.sh",
    "./scripts/register-openclaw-operator-signal-cron.sh --dry-run",
    "./scripts/register-openclaw-operator-signal-cron.sh --print-run-command",
    "./scripts/register-openclaw-operator-signal-cron.sh --print-message",
    "./scripts/register-openclaw-operator-signal-cron.sh --action show",
    "./scripts/register-openclaw-operator-signal-cron.sh --policy-profile default"
  ]'
}

osc_powershell_register_examples_json() {
  jq -cn '[
    "powershell -ExecutionPolicy Bypass -File .\\scripts\\register-openclaw-operator-signal-cron.ps1 -Action install",
    "powershell -ExecutionPolicy Bypass -File .\\scripts\\register-openclaw-operator-signal-cron.ps1 -Action install -PolicyProfile default",
    "powershell -ExecutionPolicy Bypass -File .\\scripts\\register-openclaw-operator-signal-cron.ps1 -Action install -PolicyBundle baseline",
    "powershell -ExecutionPolicy Bypass -File .\\scripts\\register-openclaw-operator-signal-cron.ps1 -Action show",
    "powershell -ExecutionPolicy Bypass -File .\\scripts\\register-openclaw-operator-signal-cron.ps1 -Action run-now",
    "powershell -ExecutionPolicy Bypass -File .\\scripts\\register-openclaw-operator-signal-cron.ps1 -Action remove"
  ]'
}

osc_register_print_usage() {
  cat <<EOF
Usage: ${OSC_REGISTER_USAGE}

${OSC_REGISTER_SUMMARY}

Defaults:
  --action install
  --policy-profile ${OSC_OPERATOR_SIGNAL_DEFAULT_POLICY_PROFILE} (when no profile/bundle/example flags are provided)

Options:
  --action <name>                     install, show, remove, run-now
  --recreate                          On install, delete existing matching jobs first
  --dry-run                           Print the generated job payload / command without mutating cron state
  --print-run-command                 Print only the final repo-local runCommand and exit
  --print-message                     Print only the final OpenClaw cron message body and exit
  --use-entry-identity-policy-example Add the entry-identity policy bundle
  --use-monitoring-deploy-policy-example Add the monitoring-deploy policy bundle
  --policy-bundle <name>              $(osc_operator_signal_supported_policy_bundles_display) (repeatable)
  --policy-profile <name>             $(osc_operator_signal_supported_policy_profiles_display) (repeatable)
  -h, --help                          Show this help

Examples:
$(osc_register_examples_json | jq -r '.[] | "  " + .')
EOF
}

osc_install_quickstart_json() {
  jq -cn '[
    "./install-operator-signal-cron.sh",
    "./install-operator-signal-cron.sh --dry-run",
    "./install-operator-signal-cron.sh --status-json",
    "./install-operator-signal-cron.sh --doctor"
  ]'
}

osc_install_examples_json() {
  jq -cn '[
    "./install-operator-signal-cron.sh",
    "./install-operator-signal-cron.sh --dry-run",
    "./install-operator-signal-cron.sh --status",
    "./install-operator-signal-cron.sh --status-json",
    "./install-operator-signal-cron.sh --examples",
    "./install-operator-signal-cron.sh --doctor",
    "./install-operator-signal-cron.sh --doctor-json",
    "./install-operator-signal-cron.sh --help-json",
    "./install-operator-signal-cron.sh --schema",
    "./install-operator-signal-cron.sh --print-run-command",
    "./install-operator-signal-cron.sh --print-message"
  ]'
}

osc_install_shortcuts_json() {
  jq -cn '[
    {"flag":"--status","mapsTo":"--show","description":"Equivalent to the helper show action."},
    {"flag":"--status-json","description":"Print a machine-readable cron status summary and exit."},
    {"flag":"--examples","description":"Print a few common commands and exit."},
    {"flag":"--doctor","description":"Print a small environment/status summary and exit."},
    {"flag":"--doctor-json","description":"Print the same doctor snapshot as JSON and exit."},
    {"flag":"--help-json","description":"Print this help surface as JSON and exit."},
    {"flag":"--schema","description":"Print the schema catalog for the JSON surfaces and exit."},
    {"flag":"--dry-run","description":"Forward to the underlying helper dry-run."}
  ]'
}

osc_install_profiles_json() {
  jq -cn '{
    default:{summary:"Recommended starter profile",installCommand:"./install-operator-signal-cron.sh --policy-profile default"},
    identity:{summary:"Entry-identity focused profile",installCommand:"./install-operator-signal-cron.sh --policy-profile identity"},
    deploy:{summary:"Monitoring-deploy focused profile",installCommand:"./install-operator-signal-cron.sh --policy-profile deploy"}
  }'
}

osc_operator_signal_recommended_consumption_core_json() {
  local summary_meta_json
  summary_meta_json="$(osc_operator_signal_recommended_consumption_summary_meta_json)"

  jq -cn \
    --argjson summaryMeta "$summary_meta_json" \
    '{
    recommendedFirstFields:["kind","schemaVersion","contracts","surfaces","evolution.compatibilityStatus","evolution.breakingChanges"],
    recommendedStrictnessLevel:"standard",
    recommendedIntegrationProfile:"catalog-discovery",
    exampleConsumerIds:["minimal-contract-check","host-aware-powershell-check","command-detail-followup"],
    parsingHints:["Ignore unknown fields.","Prefer field-aware comparisons over full-object equality.","Check coverage before assuming host/runtime-backed behavior."],
    strictnessProfileDisplay:"standard / catalog-discovery",
    firstFieldsDisplay:"kind, schemaVersion, contracts, surfaces, evolution.compatibilityStatus, evolution.breakingChanges",
    exampleConsumersDisplay:"minimal-contract-check, host-aware-powershell-check, command-detail-followup",
    summaryMeta:$summaryMeta
  }'
}

osc_operator_signal_recommended_consumption_summary_meta_json() {
  jq -cn '{
    summaryFields:["summaryDisplay","compactSummary","firstFieldsDisplay","exampleConsumersDisplay"],
    primaryField:"summaryDisplay",
    compactField:"compactSummary",
    helpTextFields:["summaryDisplay","firstFieldsDisplay","exampleConsumersDisplay"],
    doctorTextFields:["summaryDisplay","compactSummary","firstFieldsDisplay","exampleConsumersDisplay"],
    fieldPurposes:{
      summaryDisplay:"Short human-readable summary for help, docs, and glance surfaces.",
      compactSummary:"Single-line compact summary for terse logs, grep, or compact outputs.",
      firstFieldsDisplay:"Comma-separated short display mirror of recommendedFirstFields.",
      exampleConsumersDisplay:"Comma-separated short display mirror of exampleConsumerIds."
    }
  }'
}

osc_operator_signal_recommended_consumption_summary_contract_json() {
  jq -cn '{
    fields:{summaryDisplay:"string",compactSummary:"string",firstFieldsDisplay:"string",exampleConsumersDisplay:"string",summaryMeta:"object"},
    summaryMetaFields:{summaryFields:"string[]",primaryField:"string",compactField:"string",helpTextFields:"string[]",doctorTextFields:"string[]",fieldPurposes:"object"},
    fieldPurposeKeys:{summaryDisplay:"string",compactSummary:"string",firstFieldsDisplay:"string",exampleConsumersDisplay:"string"}
  }'
}

osc_operator_signal_recommended_consumption_summary_discoverability_json() {
  local summary_meta_path="$1"

  jq -cn \
    --arg summaryMetaPath "$summary_meta_path" \
    '{
      summaryDiscoverabilityContractPath:"contracts.recommendedConsumptionSummaryDiscoverability",
      summaryContractPath:"contracts.recommendedConsumptionSummary",
      summaryMetaPath:$summaryMetaPath,
      repoRootSchemaCommand:"./install-operator-signal-cron.sh --schema",
      catalogSchemaCommand:"./scripts/read-operator-signal-cron-catalog.sh --schema",
      primaryField:"summaryDisplay",
      compactField:"compactSummary",
      recommendedLookupOrder:["summaryDiscoverabilityContractPath","summaryContractPath","summaryMetaPath","primaryField","compactField"]
    }'
}

osc_operator_signal_recommended_consumption_summary_discoverability_contract_json() {
  jq -cn '{
    fields:{summaryDiscoverabilityContractPath:"string",summaryContractPath:"string",summaryMetaPath:"string",repoRootSchemaCommand:"string",catalogSchemaCommand:"string",primaryField:"string",compactField:"string",recommendedLookupOrder:"string[]"}
  }'
}

osc_operator_signal_print_recommended_consumption_lines() {
  local json_input="$1"
  jq -r '"  summary               " + .summaryDisplay, "  first fields          " + .firstFieldsDisplay, "  example consumers     " + .exampleConsumersDisplay' <<<"$json_input"
}

osc_operator_signal_print_summary_layer_lines() {
  local summary_discoverability_json summary_surface_guide_json read_order_display
  summary_discoverability_json="$(osc_operator_signal_summary_discoverability_json)"
  summary_surface_guide_json="$(osc_operator_signal_summary_surface_guide_json)"
  read_order_display="$(jq -r '.recommendedConsumption.preferredReadOrder | map(.target) | join(" -> ")' <<<"$summary_discoverability_json")"

  printf '  entry path            %s\n' "$(jq -r '.recommendedConsumption.preferredStartPath' <<<"$summary_discoverability_json")"
  printf '  entry contract        %s\n' "$(jq -r '.contractPath' <<<"$summary_discoverability_json")"
  printf '  surface guide         %s\n' "$(jq -r '.recommendedConsumption.preferredStartPath' <<<"$summary_surface_guide_json")"
  printf '  guide contract        %s\n' "$(jq -r '.contractPath' <<<"$summary_surface_guide_json")"
  printf '  preferred read order  %s\n' "$read_order_display"
}

osc_operator_signal_print_meta_layer_lines() {
  local target="$1"
  local recommended_consumption_paths_json meta_path current_instance_path current_contract_path
  recommended_consumption_paths_json="$(osc_operator_signal_recommended_consumption_paths_json)"
  meta_path="$(jq -r --arg target "$target" '.meta[$target]' <<<"$recommended_consumption_paths_json")"
  current_instance_path="$(jq -r --arg target "$target" '.instances[$target]' <<<"$recommended_consumption_paths_json")"
  current_contract_path="$(jq -r --arg target "$target" '.contracts[$target]' <<<"$recommended_consumption_paths_json")"

  printf '  current meta path     %s\n' "$meta_path"
  printf '  meta contract         %s\n' "contracts.recommendedConsumptionMeta"
  printf '  meta discoverability  %s\n' "$(jq -r --arg target "$target" '.metaDiscoverability[$target]' <<<"$recommended_consumption_paths_json")"
  printf '  discoverability ctr   %s\n' "contracts.recommendedConsumptionMetaDiscoverability"
  printf '  current instance      %s\n' "$current_instance_path"
  printf '  current contract      %s\n' "$current_contract_path"
}

osc_install_recommended_consumption_json() {
  local core_json summary_discoverability_json recommended_consumption_paths_json
  core_json="$(osc_operator_signal_recommended_consumption_core_json)"
  summary_discoverability_json="$(osc_operator_signal_recommended_consumption_summary_discoverability_json "cli.repoRootInstall.recommendedConsumption.summaryMeta")"
  recommended_consumption_paths_json="$(osc_operator_signal_recommended_consumption_paths_json)"

  jq -cn \
    --arg schemaCommand "./install-operator-signal-cron.sh --schema" \
    --arg catalogCommand "./scripts/read-operator-signal-cron-catalog.sh --compact" \
    --arg catalogSchemaCommand "./scripts/read-operator-signal-cron-catalog.sh --schema" \
    --argjson core "$core_json" \
    --argjson summaryDiscoverability "$summary_discoverability_json" \
    --argjson recommendedConsumptionPaths "$recommended_consumption_paths_json" \
    '$core + {schemaCommand:$schemaCommand,catalogCommand:$catalogCommand,catalogSchemaCommand:$catalogSchemaCommand,summaryDisplay:("schema " + $schemaCommand + ", catalog " + $catalogCommand + ", " + $core.strictnessProfileDisplay),compactSummary:("schema:" + $schemaCommand + "|catalog:" + $catalogCommand + "|strictness:" + $core.recommendedStrictnessLevel + "|profile:" + $core.recommendedIntegrationProfile),summaryDiscoverability:$summaryDiscoverability,metaPaths:$recommendedConsumptionPaths.meta,metaContractPath:"contracts.recommendedConsumptionMeta",topLevelSummaryPaths:$recommendedConsumptionPaths.topLevelSummary,topLevelSummaryContractPaths:$recommendedConsumptionPaths.topLevelSummaryContracts,summarySurfaceGuideContractPath:"contracts.summarySurfaceGuide"}'
}

osc_install_print_examples() {
  osc_install_examples_json | jq -r '.[]'
}

osc_install_print_usage() {
  cat <<EOF
Usage: ${OSC_INSTALL_USAGE}

${OSC_INSTALL_SUMMARY}

Quickstart:
$(osc_install_quickstart_json | jq -r '.[] | "  " + .')

Shortcuts:
  --status       Equivalent to --show
  --status-json  Print a machine-readable cron status summary and exit
  --examples     Print a few common commands and exit
  --doctor       Print a small environment/status summary and exit
  --doctor-json  Print the same doctor snapshot as JSON and exit
  --help-json    Print a machine-readable help/quickstart summary and exit
  --schema       Print a machine-readable schema catalog for JSON surfaces and exit
  --dry-run      Forward to the underlying helper

Recommended consumption:
$(osc_operator_signal_print_recommended_consumption_lines "$(osc_install_recommended_consumption_json)")

Per-CLI self-description layer:
$(osc_operator_signal_print_meta_layer_lines "repoRootInstall")

Top-level summary layer:
$(osc_operator_signal_print_summary_layer_lines)

Examples:
$(osc_install_examples_json | jq -r '.[] | "  " + .')
EOF
}

osc_install_print_help_json() {
  local helper="$1"
  local quickstart_json examples_json shortcuts_json profiles_json recommended_consumption_json recommended_consumption_meta_contract_json recommended_consumption_meta_discoverability_json summary_discoverability_json summary_discoverability_contract_json summary_surface_guide_json summary_surface_guide_contract_json
  quickstart_json="$(osc_install_quickstart_json)"
  examples_json="$(osc_install_examples_json)"
  shortcuts_json="$(osc_install_shortcuts_json)"
  profiles_json="$(osc_install_profiles_json)"
  recommended_consumption_json="$(osc_install_recommended_consumption_json)"
  recommended_consumption_meta_contract_json="$(osc_operator_signal_recommended_consumption_meta_contract_json)"
  recommended_consumption_meta_discoverability_json="$(osc_operator_signal_recommended_consumption_meta_discoverability_json "repoRootInstall")"
  summary_discoverability_json="$(osc_operator_signal_summary_discoverability_json)"
  summary_discoverability_contract_json="$(osc_operator_signal_summary_discoverability_contract_json)"
  summary_surface_guide_json="$(osc_operator_signal_summary_surface_guide_json)"
  summary_surface_guide_contract_json="$(osc_operator_signal_summary_surface_guide_contract_json)"

  jq -cn \
    --arg kind "$OSC_INSTALL_HELP_KIND" \
    --argjson schemaVersion "$OSC_INSTALL_HELP_SCHEMA_VERSION" \
    --arg usage "$OSC_INSTALL_USAGE" \
    --arg summary "$OSC_INSTALL_SUMMARY" \
    --arg helper "$helper" \
    --arg defaultAction "$OSC_INSTALL_DEFAULT_ACTION" \
    --arg defaultPolicyProfile "$OSC_INSTALL_DEFAULT_POLICY_PROFILE" \
    --argjson quickstart "$quickstart_json" \
    --argjson examples "$examples_json" \
    --argjson shortcuts "$shortcuts_json" \
    --argjson profiles "$profiles_json" \
    --argjson recommendedConsumption "$recommended_consumption_json" \
    --argjson recommendedConsumptionMetaContract "$recommended_consumption_meta_contract_json" \
    --argjson recommendedConsumptionMetaDiscoverability "$recommended_consumption_meta_discoverability_json" \
    --argjson summaryDiscoverability "$summary_discoverability_json" \
    --argjson summaryDiscoverabilityContract "$summary_discoverability_contract_json" \
    --argjson summarySurfaceGuide "$summary_surface_guide_json" \
    --argjson summarySurfaceGuideContract "$summary_surface_guide_contract_json" \
    '{kind:$kind,schemaVersion:$schemaVersion,usage:$usage,summary:$summary,helper:$helper,defaults:{action:$defaultAction,policyProfile:$defaultPolicyProfile},quickstart:$quickstart,shortcuts:$shortcuts,examples:$examples,profiles:$profiles,recommendedConsumption:$recommendedConsumption,recommendedConsumptionMetaPath:"cli.repoRootInstall.recommendedConsumptionMeta",recommendedConsumptionMetaContractPath:"contracts.recommendedConsumptionMeta",recommendedConsumptionMetaContract:$recommendedConsumptionMetaContract,recommendedConsumptionMetaDiscoverabilityPath:"cli.repoRootInstall.recommendedConsumptionMeta.metaDiscoverability",recommendedConsumptionMetaDiscoverabilityContractPath:"contracts.recommendedConsumptionMetaDiscoverability",recommendedConsumptionMetaDiscoverability:$recommendedConsumptionMetaDiscoverability,catalogSummaryDiscoverabilityPath:"summaryDiscoverability.recommendedConsumption",catalogSummaryDiscoverabilityContractPath:"contracts.summaryDiscoverability",catalogSummaryDiscoverability:$summaryDiscoverability,catalogSummaryDiscoverabilityContract:$summaryDiscoverabilityContract,catalogSummarySurfaceGuidePath:"summarySurfaceGuide.recommendedConsumption",catalogSummarySurfaceGuideContractPath:"contracts.summarySurfaceGuide",catalogSummarySurfaceGuide:$summarySurfaceGuide,catalogSummarySurfaceGuideContract:$summarySurfaceGuideContract}'
}

osc_install_print_schema_json() {
  local recommended_consumption_json recommended_consumption_contract_json recommended_consumption_meta_contract_json recommended_consumption_meta_discoverability_json recommended_consumption_meta_discoverability_contract_json recommended_consumption_paths_json recommended_consumption_summary_contract_json recommended_consumption_summary_discoverability_contract_json summary_discoverability_json summary_discoverability_contract_json summary_surface_guide_json summary_surface_guide_contract_json
  recommended_consumption_json="$(osc_install_recommended_consumption_json)"
  recommended_consumption_contract_json="$(osc_operator_signal_recommended_consumption_contract_json)"
  recommended_consumption_meta_contract_json="$(osc_operator_signal_recommended_consumption_meta_contract_json)"
  recommended_consumption_meta_discoverability_json="$(osc_operator_signal_recommended_consumption_meta_discoverability_json "repoRootInstall")"
  recommended_consumption_meta_discoverability_contract_json="$(osc_operator_signal_recommended_consumption_meta_discoverability_contract_json)"
  recommended_consumption_paths_json="$(osc_operator_signal_recommended_consumption_paths_json)"
  recommended_consumption_summary_contract_json="$(osc_operator_signal_recommended_consumption_summary_contract_json)"
  recommended_consumption_summary_discoverability_contract_json="$(osc_operator_signal_recommended_consumption_summary_discoverability_contract_json)"
  summary_discoverability_json="$(osc_operator_signal_summary_discoverability_json)"
  summary_discoverability_contract_json="$(osc_operator_signal_summary_discoverability_contract_json)"
  summary_surface_guide_json="$(osc_operator_signal_summary_surface_guide_json)"
  summary_surface_guide_contract_json="$(osc_operator_signal_summary_surface_guide_contract_json)"

  jq -cn \
    --arg kind "$OSC_INSTALL_SCHEMA_KIND" \
    --argjson schemaVersion "$OSC_INSTALL_SCHEMA_VERSION" \
    --arg helpKind "$OSC_INSTALL_HELP_KIND" \
    --argjson helpSchemaVersion "$OSC_INSTALL_HELP_SCHEMA_VERSION" \
    --arg statusKind "$OSC_INSTALL_STATUS_KIND" \
    --argjson statusSchemaVersion "$OSC_INSTALL_STATUS_SCHEMA_VERSION" \
    --arg doctorKind "$OSC_INSTALL_DOCTOR_KIND" \
    --argjson doctorSchemaVersion "$OSC_INSTALL_DOCTOR_SCHEMA_VERSION" \
    --arg catalogKind "$OSC_OPERATOR_SIGNAL_CATALOG_KIND" \
    --argjson catalogSchemaVersion "$OSC_OPERATOR_SIGNAL_CATALOG_SCHEMA_VERSION" \
    --arg catalogReader "./scripts/read-operator-signal-cron-catalog.sh" \
    --arg catalogReaderHelpKind "$OSC_OPERATOR_SIGNAL_CATALOG_READER_HELP_KIND" \
    --argjson catalogReaderHelpSchemaVersion "$OSC_OPERATOR_SIGNAL_CATALOG_READER_HELP_SCHEMA_VERSION" \
    --arg catalogReaderSchemaKind "$OSC_OPERATOR_SIGNAL_CATALOG_READER_SCHEMA_KIND" \
    --argjson catalogReaderSchemaVersion "$OSC_OPERATOR_SIGNAL_CATALOG_READER_SCHEMA_VERSION" \
    --argjson recommendedConsumption "$recommended_consumption_json" \
    --argjson recommendedConsumptionContract "$recommended_consumption_contract_json" \
    --argjson recommendedConsumptionMetaContract "$recommended_consumption_meta_contract_json" \
    --argjson recommendedConsumptionMetaDiscoverability "$recommended_consumption_meta_discoverability_json" \
    --argjson recommendedConsumptionMetaDiscoverabilityContract "$recommended_consumption_meta_discoverability_contract_json" \
    --argjson recommendedConsumptionPaths "$recommended_consumption_paths_json" \
    --argjson recommendedConsumptionSummaryContract "$recommended_consumption_summary_contract_json" \
    --argjson recommendedConsumptionSummaryDiscoverabilityContract "$recommended_consumption_summary_discoverability_contract_json" \
    --argjson summaryDiscoverability "$summary_discoverability_json" \
    --argjson summaryDiscoverabilityContract "$summary_discoverability_contract_json" \
    --argjson summarySurfaceGuide "$summary_surface_guide_json" \
    --argjson summarySurfaceGuideContract "$summary_surface_guide_contract_json" \
    '{
      kind:$kind,
      schemaVersion:$schemaVersion,
      surfaces:{
        helpJson:{
          kind:$helpKind,
          schemaVersion:$helpSchemaVersion,
          summary:"Machine-readable help/quickstart surface for the repo-root install alias.",
          fields:{
            kind:"string",
            schemaVersion:"number",
            usage:"string",
            summary:"string",
            helper:"string",
            defaults:"object",
            quickstart:"string[]",
            shortcuts:"object[]",
            examples:"string[]",
            profiles:"object",
            recommendedConsumption:"object",
            recommendedConsumptionMetaPath:"string",
            recommendedConsumptionMetaContractPath:"string",
            recommendedConsumptionMetaContract:"object",
            recommendedConsumptionMetaDiscoverabilityPath:"string",
            recommendedConsumptionMetaDiscoverabilityContractPath:"string",
            recommendedConsumptionMetaDiscoverability:"object",
            catalogSummaryDiscoverabilityPath:"string",
            catalogSummaryDiscoverabilityContractPath:"string",
            catalogSummaryDiscoverability:"object",
            catalogSummaryDiscoverabilityContract:"object",
            catalogSummarySurfaceGuidePath:"string",
            catalogSummarySurfaceGuideContractPath:"string",
            catalogSummarySurfaceGuide:"object",
            catalogSummarySurfaceGuideContract:"object"
          }
        },
        statusJson:{
          kind:$statusKind,
          schemaVersion:$statusSchemaVersion,
          summary:"Machine-readable current cron status surface for the repo-root install alias.",
          fields:{
            kind:"string",
            schemaVersion:"number",
            repoRoot:"string",
            helper:"string",
            cronStatus:"string",
            matchingCount:"number",
            jobs:"object[]",
            statusOutput:"string"
          }
        },
        doctorJson:{
          kind:$doctorKind,
          schemaVersion:$doctorSchemaVersion,
          summary:"Machine-readable environment/status surface for the repo-root install alias.",
          dependsOn:["statusJson"],
          fields:{
            kind:"string",
            schemaVersion:"number",
            repoRoot:"string",
            helper:"string",
            defaultPolicyProfile:"string",
            openclawAvailable:"boolean",
            openclawPath:"string",
            cronStatus:"string",
            defaultRunCommand:"string",
            effectiveRunCommand:"string",
            helperArgs:"string[]",
            recommendedCommands:"object",
            profileCommands:"object",
            recommendedConsumption:"object",
            recommendedConsumptionMetaPath:"string",
            recommendedConsumptionMetaContractPath:"string",
            recommendedConsumptionMetaContract:"object",
            recommendedConsumptionMetaDiscoverabilityPath:"string",
            recommendedConsumptionMetaDiscoverabilityContractPath:"string",
            recommendedConsumptionMetaDiscoverability:"object",
            catalogSummaryDiscoverabilityPath:"string",
            catalogSummaryDiscoverabilityContractPath:"string",
            catalogSummaryDiscoverability:"object",
            catalogSummaryDiscoverabilityContract:"object",
            catalogSummarySurfaceGuidePath:"string",
            catalogSummarySurfaceGuideContractPath:"string",
            catalogSummarySurfaceGuide:"object",
            catalogSummarySurfaceGuideContract:"object",
            statusOutput:"string"
          }
        }
      },
      related:{
        catalogJson:{
          kind:$catalogKind,
          schemaVersion:$catalogSchemaVersion,
          reader:$catalogReader,
          readerSurfaces:{helpJson:{kind:$catalogReaderHelpKind,schemaVersion:$catalogReaderHelpSchemaVersion},schema:{kind:$catalogReaderSchemaKind,schemaVersion:$catalogReaderSchemaVersion},catalogJson:{kind:$catalogKind,schemaVersion:$catalogSchemaVersion}},
          summary:"Shared machine-readable catalog for the repo-root alias, script install helper, register helper, catalog reader, and documented PowerShell helper.",
          consumptionGuide:($recommendedConsumption + {
            preferredCommand:"./scripts/read-operator-signal-cron-catalog.sh --compact",
            evolutionPaths:{stabilityLevel:"evolution.stabilityLevel",stabilityNotes:"evolution.stabilityNotes",compatibilityStatus:"evolution.compatibilityStatus",breakingChanges:"evolution.breakingChanges",migrationHints:"evolution.migrationHints",consumerWarnings:"evolution.consumerWarnings",parsingPolicy:"evolution.parsingPolicy",strictnessLevels:"evolution.strictnessLevels",integrationProfiles:"evolution.integrationProfiles",consumerExamples:"evolution.consumerExamples",consumerProfiles:"evolution.consumerProfiles",recommendedConsumptionOrder:"evolution.recommendedConsumptionOrder"},
            contractPaths:$recommendedConsumptionPaths,
            metaPaths:$recommendedConsumptionPaths.meta,
            metaDiscoverabilityPaths:$recommendedConsumptionPaths.metaDiscoverability,
            summaryMetaPaths:$recommendedConsumptionPaths.summaryMeta,
            summaryDiscoverabilityPaths:$recommendedConsumptionPaths.summaryDiscoverability,
            summaryContractPaths:$recommendedConsumptionPaths.summaryContracts,
            topLevelSummaryPaths:$recommendedConsumptionPaths.topLevelSummary,
            topLevelSummaryContractPaths:$recommendedConsumptionPaths.topLevelSummaryContracts,
            powershellMirrorGuidancePaths:$recommendedConsumptionPaths.powershellMirrorGuidance,
            powershellMirrorConsumerExampleId:"powershell-mirror-guided-read",
            metaContractPath:"contracts.recommendedConsumptionMeta",
            metaDiscoverabilityContractPath:"contracts.recommendedConsumptionMetaDiscoverability",
            summaryContractPath:"contracts.recommendedConsumptionSummary",
            summaryDiscoverabilityContractPath:"contracts.recommendedConsumptionSummaryDiscoverability",
            summarySurfaceGuideContractPath:"contracts.summarySurfaceGuide"
          }),
          fields:{
            kind:"string",
            schemaVersion:"number",
            repoRoot:"string",
            defaults:"object",
            supported:"object",
            provenance:"object",
            coverage:"object",
            evolution:"object",
            capabilities:"object",
            surfaces:"object",
            contracts:"object",
            summaryDiscoverabilityPath:"string",
            summaryDiscoverabilityContractPath:"string",
            summaryDiscoverabilityContract:"object",
            summaryDiscoverability:"object",
            summarySurfaceGuidePath:"string",
            summarySurfaceGuideContractPath:"string",
            summarySurfaceGuideContract:"object",
            summarySurfaceGuide:"object",
            cli:"object"
          },
          recommendedConsumptionContract:$recommendedConsumptionContract,
          recommendedConsumptionMetaPath:"cli.repoRootInstall.recommendedConsumptionMeta",
          recommendedConsumptionMetaContractPath:"contracts.recommendedConsumptionMeta",
          recommendedConsumptionMetaContract:$recommendedConsumptionMetaContract,
          recommendedConsumptionMetaDiscoverabilityPath:"cli.repoRootInstall.recommendedConsumptionMeta.metaDiscoverability",
          recommendedConsumptionMetaDiscoverabilityContractPath:"contracts.recommendedConsumptionMetaDiscoverability",
          recommendedConsumptionMetaDiscoverability:$recommendedConsumptionMetaDiscoverability,
          recommendedConsumptionMetaDiscoverabilityContract:$recommendedConsumptionMetaDiscoverabilityContract,
          recommendedConsumptionSummaryContract:$recommendedConsumptionSummaryContract,
          recommendedConsumptionSummaryDiscoverabilityContract:$recommendedConsumptionSummaryDiscoverabilityContract,
          summaryDiscoverabilityPath:"summaryDiscoverability.recommendedConsumption",
          summaryDiscoverabilityContractPath:"contracts.summaryDiscoverability",
          summaryDiscoverability:$summaryDiscoverability,
          summaryDiscoverabilityContract:$summaryDiscoverabilityContract,
          summarySurfaceGuidePath:"summarySurfaceGuide.recommendedConsumption",
          summarySurfaceGuideContractPath:"contracts.summarySurfaceGuide",
          summarySurfaceGuide:$summarySurfaceGuide,
          summarySurfaceGuideContract:$summarySurfaceGuideContract
        }
      }
    }'
}

osc_install_print_status_text() {
  local status_output="$1"
  printf '%s\n' "$status_output"
}

osc_install_print_status_json() {
  local repo_root="$1"
  local helper="$2"
  local cron_status="$3"
  local matching_count="$4"
  local matching_jobs_json="$5"
  local status_output="$6"

  jq -cn \
    --arg kind "$OSC_INSTALL_STATUS_KIND" \
    --argjson schemaVersion "$OSC_INSTALL_STATUS_SCHEMA_VERSION" \
    --arg repoRoot "$repo_root" \
    --arg helper "$helper" \
    --arg cronStatus "$cron_status" \
    --argjson matchingCount "$matching_count" \
    --argjson jobs "$matching_jobs_json" \
    --arg statusOutput "$status_output" \
    '{kind:$kind,schemaVersion:$schemaVersion,repoRoot:$repoRoot,helper:$helper,cronStatus:$cronStatus,matchingCount:$matchingCount,jobs:$jobs,statusOutput:$statusOutput}'
}

osc_install_print_doctor_text() {
  local doctor_kind="$1"
  local doctor_schema_version="$2"
  local repo_root="$3"
  local helper="$4"
  local default_policy_profile="$5"
  local openclaw_available="$6"
  local openclaw_path="$7"
  local cron_status="$8"
  local default_run_command="$9"
  local effective_run_command="${10}"
  local recommended_install_command="${11}"
  local recommended_dry_run_command="${12}"
  local recommended_status_command="${13}"
  local recommended_doctor_command="${14}"
  local recommended_doctor_json_command="${15}"
  local profile_install_default="${16}"
  local profile_install_identity="${17}"
  local profile_install_deploy="${18}"
  local status_output="${19}"

  printf 'kind: %s\n' "$doctor_kind"
  printf 'schemaVersion: %s\n' "$doctor_schema_version"
  printf 'repoRoot: %s\n' "$repo_root"
  printf 'helper: %s\n' "$helper"
  printf 'defaultPolicyProfile: %s\n' "$default_policy_profile"
  printf 'openclawAvailable: %s\n' "$openclaw_available"
  printf 'openclawPath: %s\n' "$openclaw_path"
  printf 'cronStatus: %s\n' "$cron_status"
  printf 'defaultRunCommand: %s\n' "$default_run_command"
  printf 'effectiveRunCommand: %s\n' "$effective_run_command"
  printf 'recommendedInstallCommand: %s\n' "$recommended_install_command"
  printf 'recommendedDryRunCommand: %s\n' "$recommended_dry_run_command"
  printf 'recommendedStatusCommand: %s\n' "$recommended_status_command"
  printf 'recommendedDoctorCommand: %s\n' "$recommended_doctor_command"
  printf 'recommendedDoctorJsonCommand: %s\n' "$recommended_doctor_json_command"
  printf 'profileCommand.default: %s\n' "$profile_install_default"
  printf 'profileCommand.identity: %s\n' "$profile_install_identity"
  printf 'profileCommand.deploy: %s\n' "$profile_install_deploy"
  local recommended_consumption_json
  recommended_consumption_json="$(osc_install_recommended_consumption_json)"
  printf 'recommendedConsumption.summary: %s\n' "$(jq -r '.summaryDisplay' <<<"$recommended_consumption_json")"
  printf 'recommendedConsumption.compact: %s\n' "$(jq -r '.compactSummary' <<<"$recommended_consumption_json")"
  printf 'recommendedConsumption.firstFields: %s\n' "$(jq -r '.firstFieldsDisplay' <<<"$recommended_consumption_json")"
  printf 'recommendedConsumption.exampleConsumers: %s\n' "$(jq -r '.exampleConsumersDisplay' <<<"$recommended_consumption_json")"
  printf 'recommendedConsumption.metaPath: %s\n' "$(jq -r '.metaPaths.repoRootInstall' <<<"$recommended_consumption_json")"
  printf 'recommendedConsumption.metaContractPath: %s\n' "$(jq -r '.metaContractPath' <<<"$recommended_consumption_json")"
  printf 'recommendedConsumption.metaDiscoverabilityPath: %s\n' "$(jq -r '.metaDiscoverability.repoRootInstall' <<<"$(osc_operator_signal_recommended_consumption_paths_json)")"
  printf 'recommendedConsumption.metaDiscoverabilityContractPath: %s\n' "contracts.recommendedConsumptionMetaDiscoverability"
  printf 'recommendedConsumption.summaryEntryPath: %s\n' "$(jq -r '.recommendedConsumption.preferredStartPath' <<<"$(osc_operator_signal_summary_discoverability_json)")"
  printf 'recommendedConsumption.summaryEntryContractPath: %s\n' "$(jq -r '.contractPath' <<<"$(osc_operator_signal_summary_discoverability_json)")"
  printf 'recommendedConsumption.summarySurfaceGuidePath: %s\n' "$(jq -r '.recommendedConsumption.preferredStartPath' <<<"$(osc_operator_signal_summary_surface_guide_json)")"
  printf 'recommendedConsumption.summarySurfaceGuideContractPath: %s\n' "$(jq -r '.contractPath' <<<"$(osc_operator_signal_summary_surface_guide_json)")"
  printf 'recommendedConsumption.summaryReadOrder: %s\n' "$(jq -r '.recommendedConsumption.preferredReadOrder | map(.target) | join(" -> ")' <<<"$(osc_operator_signal_summary_discoverability_json)")"
  printf 'statusOutput: %s\n' "$status_output"
}

osc_install_print_doctor_json() {
  local repo_root="$1"
  local helper="$2"
  local default_policy_profile="$3"
  local openclaw_available="$4"
  local openclaw_path="$5"
  local cron_status="$6"
  local default_run_command="$7"
  local effective_run_command="$8"
  local recommended_install_command="$9"
  local recommended_dry_run_command="${10}"
  local recommended_status_command="${11}"
  local recommended_doctor_command="${12}"
  local recommended_doctor_json_command="${13}"
  local profile_install_default="${14}"
  local profile_install_identity="${15}"
  local profile_install_deploy="${16}"
  local status_output="${17}"
  local helper_args_json="${18}"
  local recommended_consumption_json recommended_consumption_meta_contract_json recommended_consumption_meta_discoverability_json summary_discoverability_json summary_discoverability_contract_json summary_surface_guide_json summary_surface_guide_contract_json
  recommended_consumption_json="$(osc_install_recommended_consumption_json)"
  recommended_consumption_meta_contract_json="$(osc_operator_signal_recommended_consumption_meta_contract_json)"
  recommended_consumption_meta_discoverability_json="$(osc_operator_signal_recommended_consumption_meta_discoverability_json "repoRootInstall")"
  summary_discoverability_json="$(osc_operator_signal_summary_discoverability_json)"
  summary_discoverability_contract_json="$(osc_operator_signal_summary_discoverability_contract_json)"
  summary_surface_guide_json="$(osc_operator_signal_summary_surface_guide_json)"
  summary_surface_guide_contract_json="$(osc_operator_signal_summary_surface_guide_contract_json)"

  jq -cn \
    --arg kind "$OSC_INSTALL_DOCTOR_KIND" \
    --argjson schemaVersion "$OSC_INSTALL_DOCTOR_SCHEMA_VERSION" \
    --arg repoRoot "$repo_root" \
    --arg helper "$helper" \
    --arg defaultPolicyProfile "$default_policy_profile" \
    --arg openclawAvailable "$openclaw_available" \
    --arg openclawPath "$openclaw_path" \
    --arg cronStatus "$cron_status" \
    --arg defaultRunCommand "$default_run_command" \
    --arg effectiveRunCommand "$effective_run_command" \
    --arg recommendedInstallCommand "$recommended_install_command" \
    --arg recommendedDryRunCommand "$recommended_dry_run_command" \
    --arg recommendedStatusCommand "$recommended_status_command" \
    --arg recommendedDoctorCommand "$recommended_doctor_command" \
    --arg recommendedDoctorJsonCommand "$recommended_doctor_json_command" \
    --arg profileInstallDefault "$profile_install_default" \
    --arg profileInstallIdentity "$profile_install_identity" \
    --arg profileInstallDeploy "$profile_install_deploy" \
    --arg statusOutput "$status_output" \
    --argjson helperArgs "$helper_args_json" \
    --argjson recommendedConsumption "$recommended_consumption_json" \
    --argjson recommendedConsumptionMetaContract "$recommended_consumption_meta_contract_json" \
    --argjson recommendedConsumptionMetaDiscoverability "$recommended_consumption_meta_discoverability_json" \
    --argjson summaryDiscoverability "$summary_discoverability_json" \
    --argjson summaryDiscoverabilityContract "$summary_discoverability_contract_json" \
    --argjson summarySurfaceGuide "$summary_surface_guide_json" \
    --argjson summarySurfaceGuideContract "$summary_surface_guide_contract_json" \
    '{kind:$kind,schemaVersion:$schemaVersion,repoRoot:$repoRoot,helper:$helper,defaultPolicyProfile:$defaultPolicyProfile,openclawAvailable:($openclawAvailable=="true"),openclawPath:$openclawPath,cronStatus:$cronStatus,defaultRunCommand:$defaultRunCommand,effectiveRunCommand:$effectiveRunCommand,helperArgs:$helperArgs,recommendedCommands:{install:$recommendedInstallCommand,dryRun:$recommendedDryRunCommand,status:$recommendedStatusCommand,doctor:$recommendedDoctorCommand,doctorJson:$recommendedDoctorJsonCommand},profileCommands:{default:$profileInstallDefault,identity:$profileInstallIdentity,deploy:$profileInstallDeploy},recommendedConsumption:$recommendedConsumption,recommendedConsumptionMetaPath:"cli.repoRootInstall.recommendedConsumptionMeta",recommendedConsumptionMetaContractPath:"contracts.recommendedConsumptionMeta",recommendedConsumptionMetaContract:$recommendedConsumptionMetaContract,recommendedConsumptionMetaDiscoverabilityPath:"cli.repoRootInstall.recommendedConsumptionMeta.metaDiscoverability",recommendedConsumptionMetaDiscoverabilityContractPath:"contracts.recommendedConsumptionMetaDiscoverability",recommendedConsumptionMetaDiscoverability:$recommendedConsumptionMetaDiscoverability,catalogSummaryDiscoverabilityPath:"summaryDiscoverability.recommendedConsumption",catalogSummaryDiscoverabilityContractPath:"contracts.summaryDiscoverability",catalogSummaryDiscoverability:$summaryDiscoverability,catalogSummaryDiscoverabilityContract:$summaryDiscoverabilityContract,catalogSummarySurfaceGuidePath:"summarySurfaceGuide.recommendedConsumption",catalogSummarySurfaceGuideContractPath:"contracts.summarySurfaceGuide",catalogSummarySurfaceGuide:$summarySurfaceGuide,catalogSummarySurfaceGuideContract:$summarySurfaceGuideContract,statusOutput:$statusOutput}'
}


osc_operator_signal_print_catalog_json() {
  local repo_root="${1:-}"
  local quickstart_json examples_json shortcuts_json profiles_json
  local script_install_examples_json register_examples_json powershell_register_examples_json
  local catalog_reader_examples_json bundles_json policy_profiles_json
  local install_recommended_consumption_json catalog_reader_recommended_consumption_json powershell_recommended_consumption_json
  local repo_root_meta_discoverability_json catalog_reader_meta_discoverability_json powershell_meta_discoverability_json
  local recommended_consumption_contract_json recommended_consumption_meta_contract_json recommended_consumption_meta_discoverability_contract_json recommended_consumption_paths_json recommended_consumption_summary_contract_json recommended_consumption_summary_discoverability_contract_json summary_discoverability_json summary_discoverability_contract_json summary_surface_guide_json summary_surface_guide_contract_json

  quickstart_json="$(osc_install_quickstart_json)"
  examples_json="$(osc_install_examples_json)"
  shortcuts_json="$(osc_install_shortcuts_json)"
  profiles_json="$(osc_install_profiles_json)"
  script_install_examples_json="$(osc_script_install_examples_json)"
  register_examples_json="$(osc_register_examples_json)"
  powershell_register_examples_json="$(osc_powershell_register_examples_json)"
  catalog_reader_examples_json="$(osc_operator_signal_catalog_reader_examples_json)"
  bundles_json="$(osc_operator_signal_supported_policy_bundles_json)"
  policy_profiles_json="$(osc_operator_signal_supported_policy_profiles_json)"
  install_recommended_consumption_json="$(osc_install_recommended_consumption_json)"
  catalog_reader_recommended_consumption_json="$(osc_catalog_reader_recommended_consumption_json)"
  powershell_recommended_consumption_json="$(osc_powershell_recommended_consumption_json)"
  repo_root_meta_discoverability_json="$(osc_operator_signal_recommended_consumption_meta_discoverability_json "repoRootInstall")"
  catalog_reader_meta_discoverability_json="$(osc_operator_signal_recommended_consumption_meta_discoverability_json "catalogReader")"
  powershell_meta_discoverability_json="$(osc_operator_signal_recommended_consumption_meta_discoverability_json "powershellRegister")"
  recommended_consumption_contract_json="$(osc_operator_signal_recommended_consumption_contract_json)"
  recommended_consumption_meta_contract_json="$(osc_operator_signal_recommended_consumption_meta_contract_json)"
  recommended_consumption_meta_discoverability_contract_json="$(osc_operator_signal_recommended_consumption_meta_discoverability_contract_json)"
  recommended_consumption_paths_json="$(osc_operator_signal_recommended_consumption_paths_json)"
  recommended_consumption_summary_contract_json="$(osc_operator_signal_recommended_consumption_summary_contract_json)"
  recommended_consumption_summary_discoverability_contract_json="$(osc_operator_signal_recommended_consumption_summary_discoverability_contract_json)"
  summary_discoverability_json="$(osc_operator_signal_summary_discoverability_json)"
  summary_discoverability_contract_json="$(osc_operator_signal_summary_discoverability_contract_json)"
  summary_surface_guide_json="$(osc_operator_signal_summary_surface_guide_json)"
  summary_surface_guide_contract_json="$(osc_operator_signal_summary_surface_guide_contract_json)"

  jq -cn \
    --arg kind "$OSC_OPERATOR_SIGNAL_CATALOG_KIND" \
    --argjson schemaVersion "$OSC_OPERATOR_SIGNAL_CATALOG_SCHEMA_VERSION" \
    --arg repoRoot "$repo_root" \
    --arg defaultPolicyProfile "$OSC_OPERATOR_SIGNAL_DEFAULT_POLICY_PROFILE" \
    --arg runCommandBase "$OSC_OPERATOR_SIGNAL_RUN_COMMAND_BASE" \
    --arg installSummary "$OSC_INSTALL_SUMMARY" \
    --arg installUsage "$OSC_INSTALL_USAGE" \
    --arg installDefaultAction "$OSC_INSTALL_DEFAULT_ACTION" \
    --arg scriptInstallSummary "$OSC_SCRIPT_INSTALL_SUMMARY" \
    --arg scriptInstallUsage "$OSC_SCRIPT_INSTALL_USAGE" \
    --arg registerSummary "$OSC_REGISTER_SUMMARY" \
    --arg registerUsage "$OSC_REGISTER_USAGE" \
    --arg powershellRegisterSummary "$OSC_POWERSHELL_REGISTER_SUMMARY" \
    --arg powershellRegisterUsage "$OSC_POWERSHELL_REGISTER_USAGE" \
    --arg catalogReaderSummary "$OSC_OPERATOR_SIGNAL_CATALOG_READER_SUMMARY" \
    --arg catalogReaderUsage "$OSC_OPERATOR_SIGNAL_CATALOG_READER_USAGE" \
    --arg helpKind "$OSC_INSTALL_HELP_KIND" \
    --argjson helpSchemaVersion "$OSC_INSTALL_HELP_SCHEMA_VERSION" \
    --arg statusKind "$OSC_INSTALL_STATUS_KIND" \
    --argjson statusSchemaVersion "$OSC_INSTALL_STATUS_SCHEMA_VERSION" \
    --arg doctorKind "$OSC_INSTALL_DOCTOR_KIND" \
    --argjson doctorSchemaVersion "$OSC_INSTALL_DOCTOR_SCHEMA_VERSION" \
    --arg schemaKind "$OSC_INSTALL_SCHEMA_KIND" \
    --argjson schemaSurfaceVersion "$OSC_INSTALL_SCHEMA_VERSION" \
    --arg catalogReaderHelpKind "$OSC_OPERATOR_SIGNAL_CATALOG_READER_HELP_KIND" \
    --argjson catalogReaderHelpSchemaVersion "$OSC_OPERATOR_SIGNAL_CATALOG_READER_HELP_SCHEMA_VERSION" \
    --arg catalogReaderSchemaKind "$OSC_OPERATOR_SIGNAL_CATALOG_READER_SCHEMA_KIND" \
    --argjson catalogReaderSchemaVersion "$OSC_OPERATOR_SIGNAL_CATALOG_READER_SCHEMA_VERSION" \
    --argjson policyBundles "$bundles_json" \
    --argjson policyProfiles "$policy_profiles_json" \
    --argjson repoRootQuickstart "$quickstart_json" \
    --argjson repoRootExamples "$examples_json" \
    --argjson repoRootShortcuts "$shortcuts_json" \
    --argjson repoRootProfiles "$profiles_json" \
    --argjson scriptInstallExamples "$script_install_examples_json" \
    --argjson registerExamples "$register_examples_json" \
    --argjson powershellRegisterExamples "$powershell_register_examples_json" \
    --argjson catalogReaderExamples "$catalog_reader_examples_json" \
    --argjson installRecommendedConsumption "$install_recommended_consumption_json" \
    --argjson catalogReaderRecommendedConsumption "$catalog_reader_recommended_consumption_json" \
    --argjson powershellRecommendedConsumption "$powershell_recommended_consumption_json" \
    --argjson repoRootMetaDiscoverability "$repo_root_meta_discoverability_json" \
    --argjson catalogReaderMetaDiscoverability "$catalog_reader_meta_discoverability_json" \
    --argjson powershellMetaDiscoverability "$powershell_meta_discoverability_json" \
    --argjson recommendedConsumptionContract "$recommended_consumption_contract_json" \
    --argjson recommendedConsumptionMetaContract "$recommended_consumption_meta_contract_json" \
    --argjson recommendedConsumptionMetaDiscoverabilityContract "$recommended_consumption_meta_discoverability_contract_json" \
    --argjson recommendedConsumptionPaths "$recommended_consumption_paths_json" \
    --argjson recommendedConsumptionSummaryContract "$recommended_consumption_summary_contract_json" \
    --argjson recommendedConsumptionSummaryDiscoverabilityContract "$recommended_consumption_summary_discoverability_contract_json" \
    --argjson summaryDiscoverability "$summary_discoverability_json" \
    --argjson summaryDiscoverabilityContract "$summary_discoverability_contract_json" \
    --argjson summarySurfaceGuide "$summary_surface_guide_json" \
    --argjson summarySurfaceGuideContract "$summary_surface_guide_contract_json" \
    '{
      kind:$kind,
      schemaVersion:$schemaVersion,
      repoRoot:$repoRoot,
      defaults:{
        policyProfile:$defaultPolicyProfile,
        runCommandBase:$runCommandBase,
        action:{repoRootInstall:$installDefaultAction,scriptInstall:"install",register:"install",powershellRegister:"install",catalogReader:"read"}
      },
      supported:{policyBundles:$policyBundles,policyProfiles:$policyProfiles},
      provenance:{
        sourceFiles:["scripts/operator-signal-cron-cli-surfaces.sh","scripts/read-operator-signal-cron-catalog.sh","install-operator-signal-cron.sh","scripts/install-operator-signal-cron.sh","scripts/register-openclaw-operator-signal-cron.sh","scripts/register-openclaw-operator-signal-cron.ps1"],
        generatedBy:{helper:"scripts/operator-signal-cron-cli-surfaces.sh",reader:"scripts/read-operator-signal-cron-catalog.sh"},
        advertisedBy:{repoRootSchema:"./install-operator-signal-cron.sh --schema",catalogReader:"./scripts/read-operator-signal-cron-catalog.sh --schema"},
        notes:["PowerShell helper metadata is currently sourced from docs/schema/capabilities alignment because this host lacks pwsh runtime verification."]
      },
      coverage:{
        repoRootInstall:{mode:"runtime-backed",hostExecutable:true,smokeValidated:true},
        scriptInstall:{mode:"runtime-backed",hostExecutable:true,smokeValidated:true},
        register:{mode:"runtime-backed",hostExecutable:true,smokeValidated:true},
        powershellRegister:{mode:"docs-schema-mirror",hostExecutable:false,smokeValidated:false,blocker:"Current host lacks pwsh"},
        catalogReader:{mode:"runtime-backed",hostExecutable:true,smokeValidated:true}
      },
      evolution:{
        historyVersion:1,
        currentStage:"recommended-consumption-powershell-mirror-consumption-guide",
        compatibilityPolicy:"additive-only-so-far",
        compatibilityStatus:"backward-compatible",
        stabilityLevel:"stable-core-additive-growth",
        stabilityNotes:[
          "kind/schemaVersion and contracts/surfaces are the preferred long-lived machine-readable anchors.",
          "The shared catalog may continue to grow additively, especially inside cli.* detail blocks and evolution guidance fields."
        ],
        compatibilityNotes:[
          "All recorded changes so far are additive.",
          "Consumers can ignore unknown fields and continue relying on kind/schemaVersion plus existing surfaces/contracts."
        ],
        breakingChanges:[],
        migrationHints:[
          "Prefer top-level provenance/coverage/evolution/capabilities/surfaces/contracts when you need manifest-style summaries.",
          "Prefer top-level summaryDiscoverability.recommendedConsumption when you only need the short-summary discovery layer.",
          "Prefer cli.*.recommendedConsumptionMeta.metaDiscoverability when you only need the lightweight per-CLI self-description discovery layer.",
          "For PowerShell docs/schema mirrors, prefer cli.powershellRegister.recommendedConsumption.recommendedDocsMirrorReadOrder before following deeper guidance.",
          "Prefer cli.* blocks when you need per-command usage/examples/defaults details."
        ],
        consumerWarnings:[
          "Do not treat documented PowerShell helper metadata as runtime-verified on this host; check coverage.powershellRegister before relying on it.",
          "Prefer ignoring unknown fields instead of asserting full-object equality on the catalog JSON.",
          "Treat cli.* usage/examples as descriptive command detail, not as the primary compatibility contract."
        ],
        parsingPolicy:{
          unknownFields:"ignore",
          preferredAnchors:["kind","schemaVersion","contracts","surfaces"],
          compareMode:"field-aware",
          avoid:["full-object-equality","host-assumptions-without-coverage-check"],
          validateBeforeDeepUse:["evolution.compatibilityStatus","evolution.breakingChanges","coverage"]
        },
        strictnessLevels:[
          {id:"lenient",summary:"For exploratory readers and dashboards that should tolerate additive growth.",rules:{unknownFields:"ignore",breakingChangesGate:false,coverageCheck:"recommended"}},
          {id:"standard",summary:"Default for automation that relies on stable contracts and compatibility signals.",rules:{unknownFields:"ignore",breakingChangesGate:true,coverageCheck:"required-before-host-assumptions"}},
          {id:"strict",summary:"For audits and schema gates that want explicit checks without relying on full-object equality.",rules:{unknownFields:"ignore-but-report",breakingChangesGate:true,coverageCheck:"required",anchorAllowlist:["kind","schemaVersion","contracts","surfaces","evolution.compatibilityStatus","evolution.breakingChanges"]}}
        ],
        integrationProfiles:[
          {id:"catalog-discovery",summary:"Use repo-root schema and catalog reader schema to discover contracts before consuming details.",entryPoints:["./install-operator-signal-cron.sh --schema","./scripts/read-operator-signal-cron-catalog.sh --schema"],focus:["related.catalogJson.consumptionGuide","contracts","surfaces"]},
          {id:"automation-gate",summary:"Use the compact catalog plus compatibility and coverage checks for stable automation gates.",entryPoints:["./scripts/read-operator-signal-cron-catalog.sh --compact"],focus:["kind","schemaVersion","evolution.compatibilityStatus","evolution.breakingChanges","coverage"]},
          {id:"operator-debug",summary:"Use provenance, coverage, capabilities, and cli detail blocks for troubleshooting after contract checks.",entryPoints:["./scripts/read-operator-signal-cron-catalog.sh --compact"],focus:["provenance","coverage","capabilities","cli"]}
        ],
        consumerExamples:[
          {id:"minimal-contract-check",summary:"Smallest safe machine-readable check before deeper parsing.",readPaths:["kind","schemaVersion","evolution.compatibilityStatus","evolution.breakingChanges"]},
          {id:"host-aware-powershell-check",summary:"Check coverage before trusting documented PowerShell helper metadata on this host.",readPaths:["coverage.powershellRegister","cli.powershellRegister.notes"]},
          {id:"powershell-mirror-guided-read",summary:"Use the PowerShell mirror machine-readable guidance before assuming runtime backing.",readPaths:["cli.powershellRegister.recommendedConsumption.metaDiscoverabilityStartPath","cli.powershellRegister.recommendedConsumption.recommendedDocsMirrorReadOrder","coverage.powershellRegister","cli.powershellRegister.notes"]},
          {id:"command-detail-followup",summary:"Discover contracts and surfaces first, then inspect per-command detail blocks.",readPaths:["contracts","surfaces","cli.repoRootInstall","cli.catalogReader"]}
        ],
        consumerProfiles:[
          {id:"automation",summary:"For scripts and gates that need the most stable machine-readable entry points.",prefer:["kind","schemaVersion","contracts","surfaces","evolution.compatibilityStatus","evolution.breakingChanges"]},
          {id:"manifest-summary",summary:"For callers that want high-level repo-local capability, provenance, and coverage summaries.",prefer:["provenance","coverage","evolution","capabilities","surfaces","contracts"]},
          {id:"command-detail",summary:"For callers that need per-command usage/examples/defaults after discovering the higher-level manifest blocks.",prefer:["cli.repoRootInstall","cli.scriptInstall","cli.register","cli.catalogReader","cli.powershellRegister"]}
        ],
        recommendedConsumptionOrder:[
          {order:1,fields:["kind","schemaVersion"],summary:"Check the contract identity first."},
          {order:2,fields:["contracts","surfaces"],summary:"Discover stable output contracts and concrete command surfaces."},
          {order:3,fields:["evolution.compatibilityStatus","evolution.breakingChanges","evolution.migrationHints"],summary:"Evaluate compatibility and migration posture before deeper parsing."},
          {order:4,fields:["provenance","coverage","capabilities"],summary:"Understand trust boundaries, runtime backing, and high-level abilities."},
          {order:5,fields:["cli"],summary:"Use per-command details only after the higher-level manifest blocks are understood."}
        ],
        latestChangedFields:["related.catalogJson.consumptionGuide.powershellMirrorGuidancePaths","related.catalogJson.consumptionGuide.powershellMirrorConsumerExampleId","outputs.catalogJson.recommendedConsumption.powershellMirrorGuidancePaths","outputs.catalogJson.recommendedConsumption.powershellMirrorConsumerExampleId","evolution.consumerExamples[].id=powershell-mirror-guided-read","evolution.currentStage","evolution.latestChangedFields","evolution.milestones"],
        milestones:[
          {order:1,id:"catalog-source",summary:"Established the shared catalog JSON source and reader.",adds:["defaults","supported","cli"],changedFields:["defaults","supported","cli"],compatibilityNotes:["Introduced the initial shared catalog shape."]},
          {order:2,id:"reader-self-description",summary:"Added reader help/schema self-description and repo-root schema discoverability.",adds:["cli.catalogReader","related.catalogJson.readerSurfaces"],changedFields:["cli.catalogReader","related.catalogJson.readerSurfaces"],compatibilityNotes:["Added reader discovery metadata without removing prior fields."]},
          {order:3,id:"surface-contract-aggregates",summary:"Added top-level surfaces and contracts aggregates.",adds:["surfaces","contracts"],changedFields:["surfaces","contracts"],compatibilityNotes:["Added higher-level surface and contract inventories for easier consumers."]},
          {order:4,id:"capability-aggregate",summary:"Added top-level capabilities aggregate and aligned per-CLI capability blocks.",adds:["capabilities"],changedFields:["capabilities","cli.repoRootInstall.capabilities","cli.scriptInstall.capabilities","cli.register.capabilities"],compatibilityNotes:["Added a manifest-style capability view while keeping per-CLI detail blocks."]},
          {order:5,id:"provenance-coverage",summary:"Added provenance and coverage truth blocks.",adds:["provenance","coverage"],changedFields:["provenance","coverage"],compatibilityNotes:["Made runtime-backed vs docs/schema-mirror boundaries explicit."]},
          {order:6,id:"evolution-history",summary:"Added machine-readable evolution history metadata.",adds:["evolution"],changedFields:["evolution.historyVersion","evolution.currentStage","evolution.compatibilityPolicy","evolution.compatibilityStatus","evolution.stabilityLevel","evolution.stabilityNotes","evolution.compatibilityNotes","evolution.breakingChanges","evolution.migrationHints","evolution.consumerWarnings","evolution.parsingPolicy","evolution.strictnessLevels","evolution.integrationProfiles","evolution.consumerExamples","evolution.consumerProfiles","evolution.recommendedConsumptionOrder","evolution.latestChangedFields","evolution.milestones","evolution.milestones[].changedFields","evolution.milestones[].compatibilityNotes","evolution.nextSuggested"],compatibilityNotes:["Initial evolution record is additive and intended for downstream schema-tracking consumers."]},
          {order:7,id:"recommended-consumption-self-description",summary:"Promoted per-CLI recommendedConsumption from lightweight instances to explicit contracts, path maps, and per-CLI self-description.",adds:["contracts.recommendedConsumption","related.catalogJson.consumptionGuide.contractPaths","related.catalogJson.recommendedConsumptionContract","outputs.catalogJson.recommendedConsumption","cli.repoRootInstall.recommendedConsumptionMeta","cli.catalogReader.recommendedConsumptionMeta","cli.powershellRegister.recommendedConsumptionMeta"],changedFields:["contracts.recommendedConsumption","related.catalogJson.consumptionGuide.contractPaths","related.catalogJson.recommendedConsumptionContract","outputs.catalogJson.recommendedConsumption","cli.repoRootInstall.recommendedConsumptionMeta","cli.catalogReader.recommendedConsumptionMeta","cli.powershellRegister.recommendedConsumptionMeta","evolution.currentStage","evolution.latestChangedFields","evolution.milestones"],compatibilityNotes:["This milestone is additive and keeps existing recommendedConsumption instances intact while adding explicit contract and path-discovery layers."]},
          {order:8,id:"recommended-consumption-summary-meta",summary:"Added explicit summary-meta discoverability for the compact recommendedConsumption display layer.",adds:["related.catalogJson.consumptionGuide.summaryMetaPaths","cli.repoRootInstall.recommendedConsumption.summaryMeta","cli.catalogReader.recommendedConsumption.summaryMeta","cli.powershellRegister.recommendedConsumption.summaryMeta","cli.repoRootInstall.recommendedConsumptionMeta.summaryMetaPath","cli.catalogReader.recommendedConsumptionMeta.summaryMetaPath","cli.powershellRegister.recommendedConsumptionMeta.summaryMetaPath"],changedFields:["contracts.recommendedConsumption","related.catalogJson.consumptionGuide.summaryMetaPaths","outputs.catalogJson.recommendedConsumption","cli.repoRootInstall.recommendedConsumption.summaryMeta","cli.catalogReader.recommendedConsumption.summaryMeta","cli.powershellRegister.recommendedConsumption.summaryMeta","cli.repoRootInstall.recommendedConsumptionMeta.summaryMetaPath","cli.catalogReader.recommendedConsumptionMeta.summaryMetaPath","cli.powershellRegister.recommendedConsumptionMeta.summaryMetaPath","evolution.currentStage","evolution.latestChangedFields","evolution.milestones"],compatibilityNotes:["This milestone is additive and keeps existing recommendedConsumption fields intact while making the summary layer more explicitly discoverable."]},
          {order:9,id:"recommended-consumption-summary-contract",summary:"Added a focused shared contract for the recommendedConsumption summary layer.",adds:["contracts.recommendedConsumptionSummary","related.catalogJson.consumptionGuide.summaryContractPath","related.catalogJson.recommendedConsumptionSummaryContract","outputs.catalogJson.recommendedConsumption.summaryContractPath","outputs.catalogJson.recommendedConsumption.summaryContract","cli.repoRootInstall.recommendedConsumptionMeta.summaryContractPath","cli.catalogReader.recommendedConsumptionMeta.summaryContractPath","cli.powershellRegister.recommendedConsumptionMeta.summaryContractPath"],changedFields:["contracts.recommendedConsumptionSummary","related.catalogJson.consumptionGuide.summaryContractPath","related.catalogJson.recommendedConsumptionSummaryContract","outputs.catalogJson.recommendedConsumption.summaryContractPath","outputs.catalogJson.recommendedConsumption.summaryContract","cli.repoRootInstall.recommendedConsumptionMeta.summaryContractPath","cli.catalogReader.recommendedConsumptionMeta.summaryContractPath","cli.powershellRegister.recommendedConsumptionMeta.summaryContractPath","evolution.currentStage","evolution.latestChangedFields","evolution.milestones"],compatibilityNotes:["This milestone is additive and keeps the full recommendedConsumption contracts intact while exposing a narrower contract for the short-summary layer."]},
          {order:10,id:"recommended-consumption-summary-light-entry",summary:"Added explicit light-entry summaryDiscoverability blocks for help/doctor/help-json surfaces.",adds:["contracts.recommendedConsumptionSummaryDiscoverability","related.catalogJson.consumptionGuide.summaryDiscoverabilityPaths","related.catalogJson.consumptionGuide.summaryDiscoverabilityContractPath","related.catalogJson.recommendedConsumptionSummaryDiscoverabilityContract","cli.repoRootInstall.recommendedConsumption.summaryDiscoverability","cli.catalogReader.recommendedConsumption.summaryDiscoverability","cli.powershellRegister.recommendedConsumption.summaryDiscoverability","cli.repoRootInstall.recommendedConsumptionMeta.summaryDiscoverabilityPath","cli.catalogReader.recommendedConsumptionMeta.summaryDiscoverabilityPath","cli.powershellRegister.recommendedConsumptionMeta.summaryDiscoverabilityPath","cli.repoRootInstall.recommendedConsumptionMeta.summaryDiscoverabilityContractPath","cli.catalogReader.recommendedConsumptionMeta.summaryDiscoverabilityContractPath","cli.powershellRegister.recommendedConsumptionMeta.summaryDiscoverabilityContractPath"],changedFields:["contracts.recommendedConsumptionSummaryDiscoverability","related.catalogJson.consumptionGuide.summaryDiscoverabilityPaths","related.catalogJson.consumptionGuide.summaryDiscoverabilityContractPath","related.catalogJson.recommendedConsumptionSummaryDiscoverabilityContract","cli.repoRootInstall.recommendedConsumption.summaryDiscoverability","cli.catalogReader.recommendedConsumption.summaryDiscoverability","cli.powershellRegister.recommendedConsumption.summaryDiscoverability","cli.repoRootInstall.recommendedConsumptionMeta.summaryDiscoverabilityPath","cli.catalogReader.recommendedConsumptionMeta.summaryDiscoverabilityPath","cli.powershellRegister.recommendedConsumptionMeta.summaryDiscoverabilityPath","cli.repoRootInstall.recommendedConsumptionMeta.summaryDiscoverabilityContractPath","cli.catalogReader.recommendedConsumptionMeta.summaryDiscoverabilityContractPath","cli.powershellRegister.recommendedConsumptionMeta.summaryDiscoverabilityContractPath","evolution.currentStage","evolution.latestChangedFields","evolution.milestones"],compatibilityNotes:["This milestone is additive and keeps the summary contract/meta layers intact while adding lighter entry-point discoverability."]},
          {order:11,id:"recommended-consumption-top-level-summary-discoverability",summary:"Promoted the short-summary discovery layer into a top-level catalog entry point.",adds:["summaryDiscoverability.recommendedConsumption","related.catalogJson.summaryDiscoverabilityPath","related.catalogJson.summaryDiscoverability","related.catalogJson.fields.summaryDiscoverability","outputs.catalogJson.fields.summaryDiscoverability"],changedFields:["summaryDiscoverability.recommendedConsumption","related.catalogJson.summaryDiscoverabilityPath","related.catalogJson.summaryDiscoverability","related.catalogJson.fields.summaryDiscoverability","outputs.catalogJson.fields.summaryDiscoverability","evolution.currentStage","evolution.latestChangedFields","evolution.milestones"],compatibilityNotes:["This milestone is additive and keeps all existing recommendedConsumption nested paths intact while adding a higher-level summary discovery entry point."]},
          {order:12,id:"recommended-consumption-top-level-summary-contract",summary:"Added an explicit contract for the top-level summaryDiscoverability aggregate and mirrored it into lightweight surfaces.",adds:["contracts.summaryDiscoverability","related.catalogJson.summaryDiscoverabilityContractPath","related.catalogJson.summaryDiscoverabilityContract","outputs.catalogJson.summaryDiscoverabilityContractPath","outputs.catalogJson.summaryDiscoverabilityContract","outputs.helpJson.fields.catalogSummaryDiscoverabilityContractPath","outputs.helpJson.fields.catalogSummaryDiscoverabilityContract","surfaces.helpJson.catalogSummaryDiscoverabilityContractPath","surfaces.doctorJson.catalogSummaryDiscoverabilityContractPath"],changedFields:["contracts.summaryDiscoverability","related.catalogJson.summaryDiscoverabilityContractPath","related.catalogJson.summaryDiscoverabilityContract","outputs.catalogJson.summaryDiscoverabilityContractPath","outputs.catalogJson.summaryDiscoverabilityContract","outputs.helpJson.fields.catalogSummaryDiscoverabilityContractPath","outputs.helpJson.fields.catalogSummaryDiscoverabilityContract","surfaces.helpJson.catalogSummaryDiscoverabilityContractPath","surfaces.doctorJson.catalogSummaryDiscoverabilityContractPath","evolution.currentStage","evolution.latestChangedFields","evolution.milestones"],compatibilityNotes:["This milestone is additive and keeps existing top-level summary entry points intact while adding an explicit contract and light-surface contract hints."]},
          {order:13,id:"recommended-consumption-top-level-summary-surface-guide",summary:"Added a focused top-level surface guide for where the summary layer is mirrored across compact, schema, lightweight JSON, per-CLI instances, and per-CLI self-description surfaces.",adds:["contracts.summarySurfaceGuide","summarySurfaceGuideContractPath","summarySurfaceGuideContract","summarySurfaceGuide.recommendedConsumption","summaryDiscoverability.recommendedConsumption.surfaceGuidePath","summaryDiscoverability.recommendedConsumption.surfaceGuideContractPath","summaryDiscoverability.recommendedConsumption.preferredReadOrder","contracts.recommendedConsumption.sharedCore.fields.topLevelSummaryPaths","contracts.recommendedConsumption.sharedCore.fields.topLevelSummaryContractPaths","contracts.recommendedConsumption.sharedCore.fields.summarySurfaceGuideContractPath","contracts.recommendedConsumption.repoRootInstall.fields.topLevelSummaryPaths","contracts.recommendedConsumption.repoRootInstall.fields.topLevelSummaryContractPaths","contracts.recommendedConsumption.repoRootInstall.fields.summarySurfaceGuideContractPath","contracts.recommendedConsumption.catalogReader.fields.topLevelSummaryPaths","contracts.recommendedConsumption.catalogReader.fields.topLevelSummaryContractPaths","contracts.recommendedConsumption.catalogReader.fields.summarySurfaceGuideContractPath","contracts.recommendedConsumption.powershellRegister.fields.topLevelSummaryPaths","contracts.recommendedConsumption.powershellRegister.fields.topLevelSummaryContractPaths","contracts.recommendedConsumption.powershellRegister.fields.summarySurfaceGuideContractPath","related.catalogJson.summarySurfaceGuidePath","related.catalogJson.summarySurfaceGuideContractPath","related.catalogJson.summarySurfaceGuide","related.catalogJson.summarySurfaceGuideContract","related.catalogJson.consumptionGuide.topLevelSummaryPaths","related.catalogJson.consumptionGuide.topLevelSummaryContractPaths","related.catalogJson.consumptionGuide.summarySurfaceGuideContractPath","outputs.catalogJson.summarySurfaceGuidePath","outputs.catalogJson.summarySurfaceGuideContractPath","outputs.catalogJson.summarySurfaceGuide","outputs.catalogJson.summarySurfaceGuideContract","outputs.catalogJson.recommendedConsumption.topLevelSummaryPaths","outputs.catalogJson.recommendedConsumption.topLevelSummaryContractPaths","outputs.catalogJson.recommendedConsumption.summarySurfaceGuideContractPath","cli.repoRootInstall.recommendedConsumption.topLevelSummaryPaths","cli.repoRootInstall.recommendedConsumption.topLevelSummaryContractPaths","cli.repoRootInstall.recommendedConsumption.summarySurfaceGuideContractPath","cli.catalogReader.recommendedConsumption.topLevelSummaryPaths","cli.catalogReader.recommendedConsumption.topLevelSummaryContractPaths","cli.catalogReader.recommendedConsumption.summarySurfaceGuideContractPath","cli.powershellRegister.recommendedConsumption.topLevelSummaryPaths","cli.powershellRegister.recommendedConsumption.topLevelSummaryContractPaths","cli.powershellRegister.recommendedConsumption.summarySurfaceGuideContractPath","cli.repoRootInstall.recommendedConsumptionMeta.summarySurfaceGuidePath","cli.catalogReader.recommendedConsumptionMeta.summarySurfaceGuidePath","cli.powershellRegister.recommendedConsumptionMeta.summarySurfaceGuidePath","cli.repoRootInstall.recommendedConsumptionMeta.summarySurfaceGuideContractPath","cli.catalogReader.recommendedConsumptionMeta.summarySurfaceGuideContractPath","cli.powershellRegister.recommendedConsumptionMeta.summarySurfaceGuideContractPath"],changedFields:["contracts.summarySurfaceGuide","summarySurfaceGuideContractPath","summarySurfaceGuideContract","summarySurfaceGuide.recommendedConsumption","summaryDiscoverability.recommendedConsumption.surfaceGuidePath","summaryDiscoverability.recommendedConsumption.surfaceGuideContractPath","summaryDiscoverability.recommendedConsumption.preferredReadOrder","contracts.summaryDiscoverability.recommendedConsumptionFields.preferredReadOrder","contracts.recommendedConsumption.sharedCore.fields.topLevelSummaryPaths","contracts.recommendedConsumption.sharedCore.fields.topLevelSummaryContractPaths","contracts.recommendedConsumption.sharedCore.fields.summarySurfaceGuideContractPath","contracts.recommendedConsumption.repoRootInstall.fields.topLevelSummaryPaths","contracts.recommendedConsumption.repoRootInstall.fields.topLevelSummaryContractPaths","contracts.recommendedConsumption.repoRootInstall.fields.summarySurfaceGuideContractPath","contracts.recommendedConsumption.catalogReader.fields.topLevelSummaryPaths","contracts.recommendedConsumption.catalogReader.fields.topLevelSummaryContractPaths","contracts.recommendedConsumption.catalogReader.fields.summarySurfaceGuideContractPath","contracts.recommendedConsumption.powershellRegister.fields.topLevelSummaryPaths","contracts.recommendedConsumption.powershellRegister.fields.topLevelSummaryContractPaths","contracts.recommendedConsumption.powershellRegister.fields.summarySurfaceGuideContractPath","related.catalogJson.summarySurfaceGuidePath","related.catalogJson.summarySurfaceGuideContractPath","related.catalogJson.summarySurfaceGuide","related.catalogJson.summarySurfaceGuideContract","related.catalogJson.consumptionGuide.topLevelSummaryPaths","related.catalogJson.consumptionGuide.topLevelSummaryContractPaths","related.catalogJson.consumptionGuide.summarySurfaceGuideContractPath","outputs.catalogJson.summarySurfaceGuidePath","outputs.catalogJson.summarySurfaceGuideContractPath","outputs.catalogJson.summarySurfaceGuide","outputs.catalogJson.summarySurfaceGuideContract","outputs.catalogJson.recommendedConsumption.topLevelSummaryPaths","outputs.catalogJson.recommendedConsumption.topLevelSummaryContractPaths","outputs.catalogJson.recommendedConsumption.summarySurfaceGuideContractPath","cli.repoRootInstall.recommendedConsumption.topLevelSummaryPaths","cli.repoRootInstall.recommendedConsumption.topLevelSummaryContractPaths","cli.repoRootInstall.recommendedConsumption.summarySurfaceGuideContractPath","cli.catalogReader.recommendedConsumption.topLevelSummaryPaths","cli.catalogReader.recommendedConsumption.topLevelSummaryContractPaths","cli.catalogReader.recommendedConsumption.summarySurfaceGuideContractPath","cli.powershellRegister.recommendedConsumption.topLevelSummaryPaths","cli.powershellRegister.recommendedConsumption.topLevelSummaryContractPaths","cli.powershellRegister.recommendedConsumption.summarySurfaceGuideContractPath","cli.repoRootInstall.recommendedConsumptionMeta.summarySurfaceGuidePath","cli.catalogReader.recommendedConsumptionMeta.summarySurfaceGuidePath","cli.powershellRegister.recommendedConsumptionMeta.summarySurfaceGuidePath","cli.repoRootInstall.recommendedConsumptionMeta.summarySurfaceGuideContractPath","cli.catalogReader.recommendedConsumptionMeta.summarySurfaceGuideContractPath","cli.powershellRegister.recommendedConsumptionMeta.summarySurfaceGuideContractPath","evolution.currentStage","evolution.latestChangedFields","evolution.milestones"],compatibilityNotes:["This milestone is additive and keeps the top-level summary entry and contract intact while adding a stable per-surface guide, a direct recommended read order, mirrored top-level guide hints inside per-CLI self-description metadata, symmetric schema-path maps for top-level summary discovery, and aligned per-CLI instance contracts for the same top-level path maps."]},
          {order:14,id:"recommended-consumption-meta-discoverability",summary:"Added a focused lightweight discoverability block for per-CLI recommendedConsumptionMeta and mirrored it into help/schema surfaces.",adds:["contracts.recommendedConsumptionMetaDiscoverability","related.catalogJson.consumptionGuide.metaDiscoverabilityPaths","related.catalogJson.consumptionGuide.metaDiscoverabilityContractPath","related.catalogJson.recommendedConsumptionMetaDiscoverabilityPath","related.catalogJson.recommendedConsumptionMetaDiscoverabilityContractPath","related.catalogJson.recommendedConsumptionMetaDiscoverability","related.catalogJson.recommendedConsumptionMetaDiscoverabilityContract","outputs.helpJson.fields.recommendedConsumptionMetaDiscoverabilityPath","outputs.helpJson.fields.recommendedConsumptionMetaDiscoverabilityContractPath","outputs.helpJson.fields.recommendedConsumptionMetaDiscoverability","outputs.catalogJson.recommendedConsumption.metaDiscoverabilityPaths","outputs.catalogJson.recommendedConsumption.metaDiscoverability","outputs.catalogJson.recommendedConsumption.metaDiscoverabilityContractPath","outputs.catalogJson.recommendedConsumption.metaDiscoverabilityContract","surfaces.helpJson.fields.recommendedConsumptionMetaDiscoverabilityPath","surfaces.helpJson.fields.recommendedConsumptionMetaDiscoverabilityContractPath","surfaces.helpJson.fields.recommendedConsumptionMetaDiscoverability","surfaces.doctorJson.fields.recommendedConsumptionMetaDiscoverabilityPath","surfaces.doctorJson.fields.recommendedConsumptionMetaDiscoverabilityContractPath","surfaces.doctorJson.fields.recommendedConsumptionMetaDiscoverability","cli.repoRootInstall.recommendedConsumptionMeta.metaDiscoverabilityPath","cli.repoRootInstall.recommendedConsumptionMeta.metaDiscoverabilityContractPath","cli.repoRootInstall.recommendedConsumptionMeta.metaDiscoverability","cli.catalogReader.recommendedConsumptionMeta.metaDiscoverabilityPath","cli.catalogReader.recommendedConsumptionMeta.metaDiscoverabilityContractPath","cli.catalogReader.recommendedConsumptionMeta.metaDiscoverability","cli.powershellRegister.recommendedConsumptionMeta.metaDiscoverabilityPath","cli.powershellRegister.recommendedConsumptionMeta.metaDiscoverabilityContractPath","cli.powershellRegister.recommendedConsumptionMeta.metaDiscoverability"],changedFields:["contracts.recommendedConsumptionMetaDiscoverability","related.catalogJson.consumptionGuide.metaDiscoverabilityPaths","related.catalogJson.consumptionGuide.metaDiscoverabilityContractPath","related.catalogJson.recommendedConsumptionMetaDiscoverabilityPath","related.catalogJson.recommendedConsumptionMetaDiscoverabilityContractPath","related.catalogJson.recommendedConsumptionMetaDiscoverability","related.catalogJson.recommendedConsumptionMetaDiscoverabilityContract","outputs.helpJson.fields.recommendedConsumptionMetaDiscoverabilityPath","outputs.helpJson.fields.recommendedConsumptionMetaDiscoverabilityContractPath","outputs.helpJson.fields.recommendedConsumptionMetaDiscoverability","outputs.catalogJson.recommendedConsumption.metaDiscoverabilityPaths","outputs.catalogJson.recommendedConsumption.metaDiscoverability","outputs.catalogJson.recommendedConsumption.metaDiscoverabilityContractPath","outputs.catalogJson.recommendedConsumption.metaDiscoverabilityContract","surfaces.helpJson.fields.recommendedConsumptionMetaDiscoverabilityPath","surfaces.helpJson.fields.recommendedConsumptionMetaDiscoverabilityContractPath","surfaces.helpJson.fields.recommendedConsumptionMetaDiscoverability","surfaces.doctorJson.fields.recommendedConsumptionMetaDiscoverabilityPath","surfaces.doctorJson.fields.recommendedConsumptionMetaDiscoverabilityContractPath","surfaces.doctorJson.fields.recommendedConsumptionMetaDiscoverability","cli.repoRootInstall.recommendedConsumptionMeta.metaDiscoverabilityPath","cli.repoRootInstall.recommendedConsumptionMeta.metaDiscoverabilityContractPath","cli.repoRootInstall.recommendedConsumptionMeta.metaDiscoverability","cli.catalogReader.recommendedConsumptionMeta.metaDiscoverabilityPath","cli.catalogReader.recommendedConsumptionMeta.metaDiscoverabilityContractPath","cli.catalogReader.recommendedConsumptionMeta.metaDiscoverability","cli.powershellRegister.recommendedConsumptionMeta.metaDiscoverabilityPath","cli.powershellRegister.recommendedConsumptionMeta.metaDiscoverabilityContractPath","cli.powershellRegister.recommendedConsumptionMeta.metaDiscoverability","evolution.currentStage","evolution.latestChangedFields","evolution.migrationHints","evolution.milestones"],compatibilityNotes:["This milestone is additive and keeps the existing per-CLI self-description layer intact while adding a smaller discovery block that points back to the stable meta contract, current instance contract, and adjacent summary-layer paths."]},
          {order:15,id:"recommended-consumption-powershell-mirror-guidance",summary:"Added more explicit docs/schema-mirror discovery snippets and read order for the documented PowerShell helper.",adds:["contracts.recommendedConsumption.powershellRegister.fields.notesPath","contracts.recommendedConsumption.powershellRegister.fields.metaDiscoverabilityStartPath","contracts.recommendedConsumption.powershellRegister.fields.discoveryCommands","contracts.recommendedConsumption.powershellRegister.fields.recommendedDocsMirrorReadOrder","cli.powershellRegister.capabilities.policyBundle","cli.powershellRegister.capabilities.policyProfile","cli.powershellRegister.recommendedConsumption.notesPath","cli.powershellRegister.recommendedConsumption.metaDiscoverabilityStartPath","cli.powershellRegister.recommendedConsumption.discoveryCommands","cli.powershellRegister.recommendedConsumption.recommendedDocsMirrorReadOrder","cli.powershellRegister.notes"],changedFields:["contracts.recommendedConsumption.powershellRegister.fields.notesPath","contracts.recommendedConsumption.powershellRegister.fields.metaDiscoverabilityStartPath","contracts.recommendedConsumption.powershellRegister.fields.discoveryCommands","contracts.recommendedConsumption.powershellRegister.fields.recommendedDocsMirrorReadOrder","cli.powershellRegister.capabilities.policyBundle","cli.powershellRegister.capabilities.policyProfile","cli.powershellRegister.recommendedConsumption.notesPath","cli.powershellRegister.recommendedConsumption.metaDiscoverabilityStartPath","cli.powershellRegister.recommendedConsumption.discoveryCommands","cli.powershellRegister.recommendedConsumption.recommendedDocsMirrorReadOrder","cli.powershellRegister.notes","evolution.currentStage","evolution.latestChangedFields","evolution.migrationHints","evolution.milestones"],compatibilityNotes:["This milestone is additive and keeps the existing PowerShell docs/schema mirror intact while adding clearer per-CLI discovery snippets, an explicit read order, and aligned capability metadata."]},
          {order:16,id:"recommended-consumption-powershell-mirror-consumption-guide",summary:"Mirrored the PowerShell docs/schema-mirror guidance into the narrower consumption-guide surfaces.",adds:["related.catalogJson.consumptionGuide.powershellMirrorGuidancePaths","related.catalogJson.consumptionGuide.powershellMirrorConsumerExampleId","outputs.catalogJson.recommendedConsumption.powershellMirrorGuidancePaths","outputs.catalogJson.recommendedConsumption.powershellMirrorConsumerExampleId","evolution.consumerExamples[].id=powershell-mirror-guided-read"],changedFields:["related.catalogJson.consumptionGuide.powershellMirrorGuidancePaths","related.catalogJson.consumptionGuide.powershellMirrorConsumerExampleId","outputs.catalogJson.recommendedConsumption.powershellMirrorGuidancePaths","outputs.catalogJson.recommendedConsumption.powershellMirrorConsumerExampleId","evolution.consumerExamples","evolution.currentStage","evolution.latestChangedFields","evolution.milestones"],compatibilityNotes:["This milestone is additive and keeps the existing PowerShell mirror guidance intact while surfacing the most useful paths and consumer example id directly inside the narrower consumption-guide blocks."]}
        ],
        nextSuggested:[
          "Promote powershellRegister coverage from docs-schema-mirror to runtime-backed when pwsh is available.",
          "Optionally add per-surface compatibility notes if external consumers need finer-grained schema evolution guidance."
        ]
      },
      capabilities:{
        repoRootInstall:{status:true,statusJson:true,examples:true,doctor:true,doctorJson:true,helpJson:true,schema:true,printRunCommand:true,printMessage:true,forwardPolicyFlags:true},
        scriptInstall:{dryRun:true,show:true,remove:true,runNow:true,printRunCommand:true,printMessage:true,defaultPolicyProfileInjection:true},
        register:{dryRun:true,printRunCommand:true,printMessage:true,recreate:true,policyBundle:true,policyProfile:true,useEntryIdentityPolicyExample:true,useMonitoringDeployPolicyExample:true},
        powershellRegister:{dryRun:false,printRunCommand:false,printMessage:false,recreate:true,policyBundle:true,policyProfile:true,useEntryIdentityPolicyExample:true,useMonitoringDeployPolicyExample:true},
        catalogReader:{compact:true,field:true,helpJson:true,schema:true}
      },
      surfaces:{
        repoRootInstall:{
          helpJson:{command:"./install-operator-signal-cron.sh --help-json",kind:$helpKind,schemaVersion:$helpSchemaVersion},
          statusJson:{command:"./install-operator-signal-cron.sh --status-json",kind:$statusKind,schemaVersion:$statusSchemaVersion},
          doctorJson:{command:"./install-operator-signal-cron.sh --doctor-json",kind:$doctorKind,schemaVersion:$doctorSchemaVersion},
          schema:{command:"./install-operator-signal-cron.sh --schema",kind:$schemaKind,schemaVersion:$schemaSurfaceVersion}
        },
        catalogReader:{
          helpJson:{command:"./scripts/read-operator-signal-cron-catalog.sh --help-json",kind:$catalogReaderHelpKind,schemaVersion:$catalogReaderHelpSchemaVersion},
          schema:{command:"./scripts/read-operator-signal-cron-catalog.sh --schema",kind:$catalogReaderSchemaKind,schemaVersion:$catalogReaderSchemaVersion},
          catalogJson:{command:"./scripts/read-operator-signal-cron-catalog.sh --compact",kind:$kind,schemaVersion:$schemaVersion}
        }
      },
      contracts:{
        sharedCatalog:{kind:$kind,schemaVersion:$schemaVersion},
        recommendedConsumption:$recommendedConsumptionContract,
        recommendedConsumptionMeta:$recommendedConsumptionMetaContract,
        recommendedConsumptionMetaDiscoverability:$recommendedConsumptionMetaDiscoverabilityContract,
        recommendedConsumptionSummary:$recommendedConsumptionSummaryContract,
        recommendedConsumptionSummaryDiscoverability:$recommendedConsumptionSummaryDiscoverabilityContract,
        summaryDiscoverability:$summaryDiscoverabilityContract,
        summarySurfaceGuide:$summarySurfaceGuideContract,
        repoRootInstall:{
          helpJson:{kind:$helpKind,schemaVersion:$helpSchemaVersion},
          statusJson:{kind:$statusKind,schemaVersion:$statusSchemaVersion},
          doctorJson:{kind:$doctorKind,schemaVersion:$doctorSchemaVersion},
          schema:{kind:$schemaKind,schemaVersion:$schemaSurfaceVersion}
        },
        catalogReader:{
          helpJson:{kind:$catalogReaderHelpKind,schemaVersion:$catalogReaderHelpSchemaVersion},
          schema:{kind:$catalogReaderSchemaKind,schemaVersion:$catalogReaderSchemaVersion},
          catalogJson:{kind:$kind,schemaVersion:$schemaVersion}
        }
      },
      summaryDiscoverabilityPath:"summaryDiscoverability.recommendedConsumption",
      summaryDiscoverabilityContractPath:"contracts.summaryDiscoverability",
      summaryDiscoverabilityContract:$summaryDiscoverabilityContract,
      summaryDiscoverability:$summaryDiscoverability,
      summarySurfaceGuidePath:"summarySurfaceGuide.recommendedConsumption",
      summarySurfaceGuideContractPath:"contracts.summarySurfaceGuide",
      summarySurfaceGuideContract:$summarySurfaceGuideContract,
      summarySurfaceGuide:$summarySurfaceGuide,
      cli:{
        repoRootInstall:{
          summary:$installSummary,
          usage:$installUsage,
          surfaces:{
            helpJson:{kind:$helpKind,schemaVersion:$helpSchemaVersion},
            statusJson:{kind:$statusKind,schemaVersion:$statusSchemaVersion},
            doctorJson:{kind:$doctorKind,schemaVersion:$doctorSchemaVersion},
            schema:{kind:$schemaKind,schemaVersion:$schemaSurfaceVersion}
          },
          defaults:{action:$installDefaultAction,policyProfile:$defaultPolicyProfile},
          capabilities:{status:true,statusJson:true,examples:true,doctor:true,doctorJson:true,helpJson:true,schema:true,printRunCommand:true,printMessage:true,forwardPolicyFlags:true},
          recommendedConsumption:$installRecommendedConsumption,
          recommendedConsumptionMeta:{instancePath:$recommendedConsumptionPaths.instances.repoRootInstall,contractPath:$recommendedConsumptionPaths.contracts.repoRootInstall,metaContractPath:"contracts.recommendedConsumptionMeta",summaryMetaPath:$recommendedConsumptionPaths.summaryMeta.repoRootInstall,summaryDiscoverabilityPath:$recommendedConsumptionPaths.summaryDiscoverability.repoRootInstall,summarySurfaceGuidePath:"summarySurfaceGuide.recommendedConsumption",summaryContractPath:"contracts.recommendedConsumptionSummary",summaryDiscoverabilityContractPath:"contracts.recommendedConsumptionSummaryDiscoverability",summarySurfaceGuideContractPath:"contracts.summarySurfaceGuide",metaDiscoverabilityPath:$recommendedConsumptionPaths.metaDiscoverability.repoRootInstall,metaDiscoverabilityContractPath:"contracts.recommendedConsumptionMetaDiscoverability",metaDiscoverability:$repoRootMetaDiscoverability,contract:$recommendedConsumptionContract.repoRootInstall},
          quickstart:$repoRootQuickstart,
          examples:$repoRootExamples,
          shortcuts:$repoRootShortcuts,
          profiles:$repoRootProfiles
        },
        scriptInstall:{
          summary:$scriptInstallSummary,
          usage:$scriptInstallUsage,
          defaults:{action:"install",policyProfile:$defaultPolicyProfile},
          capabilities:{dryRun:true,show:true,remove:true,runNow:true,printRunCommand:true,printMessage:true,defaultPolicyProfileInjection:true},
          examples:$scriptInstallExamples
        },
        register:{
          summary:$registerSummary,
          usage:$registerUsage,
          defaults:{action:"install",policyProfile:$defaultPolicyProfile},
          supported:{policyBundles:$policyBundles,policyProfiles:$policyProfiles},
          capabilities:{dryRun:true,printRunCommand:true,printMessage:true,recreate:true,policyBundle:true,policyProfile:true,useEntryIdentityPolicyExample:true,useMonitoringDeployPolicyExample:true},
          examples:$registerExamples
        },
        powershellRegister:{
          summary:$powershellRegisterSummary,
          usage:$powershellRegisterUsage,
          defaults:{action:"install"},
          supported:{policyBundles:$policyBundles,policyProfiles:$policyProfiles},
          capabilities:{dryRun:false,printRunCommand:false,printMessage:false,recreate:true,policyBundle:true,policyProfile:true,useEntryIdentityPolicyExample:true,useMonitoringDeployPolicyExample:true},
          recommendedConsumption:$powershellRecommendedConsumption,
          recommendedConsumptionMeta:{instancePath:$recommendedConsumptionPaths.instances.powershellRegister,contractPath:$recommendedConsumptionPaths.contracts.powershellRegister,metaContractPath:"contracts.recommendedConsumptionMeta",summaryMetaPath:$recommendedConsumptionPaths.summaryMeta.powershellRegister,summaryDiscoverabilityPath:$recommendedConsumptionPaths.summaryDiscoverability.powershellRegister,summarySurfaceGuidePath:"summarySurfaceGuide.recommendedConsumption",summaryContractPath:"contracts.recommendedConsumptionSummary",summaryDiscoverabilityContractPath:"contracts.recommendedConsumptionSummaryDiscoverability",summarySurfaceGuideContractPath:"contracts.summarySurfaceGuide",metaDiscoverabilityPath:$recommendedConsumptionPaths.metaDiscoverability.powershellRegister,metaDiscoverabilityContractPath:"contracts.recommendedConsumptionMetaDiscoverability",metaDiscoverability:$powershellMetaDiscoverability,contract:$recommendedConsumptionContract.powershellRegister},
          examples:$powershellRegisterExamples,
          notes:["Current host lacks pwsh, so alignment here is docs/schema/capabilities only.","For lightweight per-CLI self-description discovery, start from cli.powershellRegister.recommendedConsumptionMeta.metaDiscoverability and then check coverage.powershellRegister before treating anything here as runtime-verified."]
        },
        catalogReader:{
          summary:$catalogReaderSummary,
          usage:$catalogReaderUsage,
          defaults:{action:"read"},
          surfaces:{helpJson:{kind:$catalogReaderHelpKind,schemaVersion:$catalogReaderHelpSchemaVersion},schema:{kind:$catalogReaderSchemaKind,schemaVersion:$catalogReaderSchemaVersion},catalogJson:{kind:$kind,schemaVersion:$schemaVersion}},
          capabilities:{compact:true,field:true,helpJson:true,schema:true},
          recommendedConsumption:$catalogReaderRecommendedConsumption,
          recommendedConsumptionMeta:{instancePath:$recommendedConsumptionPaths.instances.catalogReader,contractPath:$recommendedConsumptionPaths.contracts.catalogReader,metaContractPath:"contracts.recommendedConsumptionMeta",summaryMetaPath:$recommendedConsumptionPaths.summaryMeta.catalogReader,summaryDiscoverabilityPath:$recommendedConsumptionPaths.summaryDiscoverability.catalogReader,summarySurfaceGuidePath:"summarySurfaceGuide.recommendedConsumption",summaryContractPath:"contracts.recommendedConsumptionSummary",summaryDiscoverabilityContractPath:"contracts.recommendedConsumptionSummaryDiscoverability",summarySurfaceGuideContractPath:"contracts.summarySurfaceGuide",metaDiscoverabilityPath:$recommendedConsumptionPaths.metaDiscoverability.catalogReader,metaDiscoverabilityContractPath:"contracts.recommendedConsumptionMetaDiscoverability",metaDiscoverability:$catalogReaderMetaDiscoverability,contract:$recommendedConsumptionContract.catalogReader},
          examples:$catalogReaderExamples
        }
      }
    }'
}


osc_operator_signal_catalog_reader_examples_json() {
  jq -cn '[
    "./scripts/read-operator-signal-cron-catalog.sh",
    "./scripts/read-operator-signal-cron-catalog.sh --compact",
    "./scripts/read-operator-signal-cron-catalog.sh --field defaults.policyProfile",
    "./scripts/read-operator-signal-cron-catalog.sh --help-json",
    "./scripts/read-operator-signal-cron-catalog.sh --schema"
  ]'
}

osc_catalog_reader_recommended_consumption_json() {
  local core_json summary_discoverability_json recommended_consumption_paths_json
  core_json="$(osc_operator_signal_recommended_consumption_core_json)"
  summary_discoverability_json="$(osc_operator_signal_recommended_consumption_summary_discoverability_json "cli.catalogReader.recommendedConsumption.summaryMeta")"
  recommended_consumption_paths_json="$(osc_operator_signal_recommended_consumption_paths_json)"

  jq -cn \
    --arg repoRootSchemaCommand "./install-operator-signal-cron.sh --schema" \
    --arg catalogCommand "./scripts/read-operator-signal-cron-catalog.sh --compact" \
    --arg readerSchemaCommand "./scripts/read-operator-signal-cron-catalog.sh --schema" \
    --argjson core "$core_json" \
    --argjson summaryDiscoverability "$summary_discoverability_json" \
    --argjson recommendedConsumptionPaths "$recommended_consumption_paths_json" \
    '$core + {repoRootSchemaCommand:$repoRootSchemaCommand,catalogCommand:$catalogCommand,readerSchemaCommand:$readerSchemaCommand,summaryDisplay:("reader schema " + $readerSchemaCommand + ", catalog " + $catalogCommand + ", " + $core.strictnessProfileDisplay),compactSummary:("reader-schema:" + $readerSchemaCommand + "|catalog:" + $catalogCommand + "|strictness:" + $core.recommendedStrictnessLevel + "|profile:" + $core.recommendedIntegrationProfile),summaryDiscoverability:$summaryDiscoverability,metaPaths:$recommendedConsumptionPaths.meta,metaContractPath:"contracts.recommendedConsumptionMeta",topLevelSummaryPaths:$recommendedConsumptionPaths.topLevelSummary,topLevelSummaryContractPaths:$recommendedConsumptionPaths.topLevelSummaryContracts,summarySurfaceGuideContractPath:"contracts.summarySurfaceGuide"}'
}

osc_powershell_recommended_consumption_json() {
  local core_json summary_discoverability_json recommended_consumption_paths_json
  core_json="$(osc_operator_signal_recommended_consumption_core_json)"
  summary_discoverability_json="$(osc_operator_signal_recommended_consumption_summary_discoverability_json "cli.powershellRegister.recommendedConsumption.summaryMeta")"
  recommended_consumption_paths_json="$(osc_operator_signal_recommended_consumption_paths_json)"

  jq -cn \
    --arg repoRootSchemaCommand "./install-operator-signal-cron.sh --schema" \
    --arg catalogCommand "./scripts/read-operator-signal-cron-catalog.sh --compact" \
    --arg powershellInstallCommand "powershell -ExecutionPolicy Bypass -File .\\scripts\\register-openclaw-operator-signal-cron.ps1 -Action install -PolicyProfile default" \
    --arg coveragePath "coverage.powershellRegister" \
    --arg notesPath "cli.powershellRegister.notes" \
    --arg metaDiscoverabilityStartPath "cli.powershellRegister.recommendedConsumptionMeta.metaDiscoverability" \
    --arg hostConstraint "Current host lacks pwsh, so use this as docs/schema guidance only unless runtime verification becomes available." \
    --argjson core "$core_json" \
    --argjson summaryDiscoverability "$summary_discoverability_json" \
    --argjson recommendedConsumptionPaths "$recommended_consumption_paths_json" \
    '$core + {repoRootSchemaCommand:$repoRootSchemaCommand,catalogCommand:$catalogCommand,powershellInstallCommand:$powershellInstallCommand,coveragePath:$coveragePath,notesPath:$notesPath,metaDiscoverabilityStartPath:$metaDiscoverabilityStartPath,docsMirrorOnly:true,hostConstraint:$hostConstraint,summaryDisplay:("powershell install " + $powershellInstallCommand + ", catalog " + $catalogCommand + ", coverage " + $coveragePath + ", " + $core.strictnessProfileDisplay),compactSummary:("pwsh-install:" + $powershellInstallCommand + "|catalog:" + $catalogCommand + "|coverage:" + $coveragePath + "|strictness:" + $core.recommendedStrictnessLevel + "|profile:" + $core.recommendedIntegrationProfile),summaryDiscoverability:$summaryDiscoverability,metaPaths:$recommendedConsumptionPaths.meta,metaContractPath:"contracts.recommendedConsumptionMeta",topLevelSummaryPaths:$recommendedConsumptionPaths.topLevelSummary,topLevelSummaryContractPaths:$recommendedConsumptionPaths.topLevelSummaryContracts,summarySurfaceGuideContractPath:"contracts.summarySurfaceGuide",discoveryCommands:{catalog:$catalogCommand,meta:("./scripts/read-operator-signal-cron-catalog.sh --field " + $metaDiscoverabilityStartPath),coverage:("./scripts/read-operator-signal-cron-catalog.sh --field " + $coveragePath),notes:("./scripts/read-operator-signal-cron-catalog.sh --field " + $notesPath)},recommendedDocsMirrorReadOrder:[{order:1,path:$metaDiscoverabilityStartPath,summary:"Read the lightweight per-CLI self-description discovery block."},{order:2,path:$coveragePath,summary:"Check whether this host still treats PowerShell as docs-schema-mirror only."},{order:3,path:$notesPath,summary:"Read the host caveat and docs-only guidance."},{order:4,path:"cli.powershellRegister.recommendedConsumption",summary:"Expand the richer PowerShell mirror guidance if more detail is needed."}]}'
}

osc_operator_signal_recommended_consumption_contract_json() {
  jq -cn '{
    sharedCore:{fields:{recommendedFirstFields:"string[]",recommendedStrictnessLevel:"string",recommendedIntegrationProfile:"string",exampleConsumerIds:"string[]",parsingHints:"string[]",strictnessProfileDisplay:"string",firstFieldsDisplay:"string",exampleConsumersDisplay:"string",summaryMeta:"object",summaryDiscoverability:"object",metaPaths:"object",metaContractPath:"string",topLevelSummaryPaths:"object",topLevelSummaryContractPaths:"object",summarySurfaceGuideContractPath:"string"}},
    repoRootInstall:{fields:{schemaCommand:"string",catalogCommand:"string",catalogSchemaCommand:"string",summaryDisplay:"string",compactSummary:"string",recommendedFirstFields:"string[]",recommendedStrictnessLevel:"string",recommendedIntegrationProfile:"string",exampleConsumerIds:"string[]",parsingHints:"string[]",strictnessProfileDisplay:"string",firstFieldsDisplay:"string",exampleConsumersDisplay:"string",summaryMeta:"object",summaryDiscoverability:"object",metaPaths:"object",metaContractPath:"string",topLevelSummaryPaths:"object",topLevelSummaryContractPaths:"object",summarySurfaceGuideContractPath:"string"}},
    catalogReader:{fields:{repoRootSchemaCommand:"string",catalogCommand:"string",readerSchemaCommand:"string",summaryDisplay:"string",compactSummary:"string",recommendedFirstFields:"string[]",recommendedStrictnessLevel:"string",recommendedIntegrationProfile:"string",exampleConsumerIds:"string[]",parsingHints:"string[]",strictnessProfileDisplay:"string",firstFieldsDisplay:"string",exampleConsumersDisplay:"string",summaryMeta:"object",summaryDiscoverability:"object",metaPaths:"object",metaContractPath:"string",topLevelSummaryPaths:"object",topLevelSummaryContractPaths:"object",summarySurfaceGuideContractPath:"string"}},
    powershellRegister:{fields:{repoRootSchemaCommand:"string",catalogCommand:"string",powershellInstallCommand:"string",coveragePath:"string",notesPath:"string",metaDiscoverabilityStartPath:"string",docsMirrorOnly:"boolean",hostConstraint:"string",summaryDisplay:"string",compactSummary:"string",recommendedFirstFields:"string[]",recommendedStrictnessLevel:"string",recommendedIntegrationProfile:"string",exampleConsumerIds:"string[]",parsingHints:"string[]",strictnessProfileDisplay:"string",firstFieldsDisplay:"string",exampleConsumersDisplay:"string",summaryMeta:"object",summaryDiscoverability:"object",metaPaths:"object",metaContractPath:"string",topLevelSummaryPaths:"object",topLevelSummaryContractPaths:"object",summarySurfaceGuideContractPath:"string",discoveryCommands:"object",recommendedDocsMirrorReadOrder:"object[]"}}
  }'
}

osc_operator_signal_recommended_consumption_meta_contract_json() {
  jq -cn '{
    fields:{instancePath:"string",contractPath:"string",metaContractPath:"string",summaryMetaPath:"string",summaryDiscoverabilityPath:"string",summarySurfaceGuidePath:"string",summaryContractPath:"string",summaryDiscoverabilityContractPath:"string",summarySurfaceGuideContractPath:"string",metaDiscoverabilityPath:"string",metaDiscoverabilityContractPath:"string",metaDiscoverability:"object",contract:"object"}
  }'
}

osc_operator_signal_recommended_consumption_meta_discoverability_contract_json() {
  jq -cn '{
    fields:{metaDiscoverabilityContractPath:"string",metaPath:"string",metaContractPath:"string",currentInstancePath:"string",currentContractPath:"string",summaryMetaPath:"string",summaryDiscoverabilityPath:"string",summaryContractPath:"string",summaryDiscoverabilityContractPath:"string",summarySurfaceGuidePath:"string",summarySurfaceGuideContractPath:"string",recommendedLookupOrder:"string[]"}
  }'
}

osc_operator_signal_recommended_consumption_meta_discoverability_json() {
  local target="$1"
  local recommended_consumption_paths_json
  recommended_consumption_paths_json="$(osc_operator_signal_recommended_consumption_paths_json)"

  jq -cn \
    --arg target "$target" \
    --argjson recommendedConsumptionPaths "$recommended_consumption_paths_json" \
    '{
      metaDiscoverabilityContractPath:"contracts.recommendedConsumptionMetaDiscoverability",
      metaPath:$recommendedConsumptionPaths.meta[$target],
      metaContractPath:"contracts.recommendedConsumptionMeta",
      currentInstancePath:$recommendedConsumptionPaths.instances[$target],
      currentContractPath:$recommendedConsumptionPaths.contracts[$target],
      summaryMetaPath:$recommendedConsumptionPaths.summaryMeta[$target],
      summaryDiscoverabilityPath:$recommendedConsumptionPaths.summaryDiscoverability[$target],
      summaryContractPath:"contracts.recommendedConsumptionSummary",
      summaryDiscoverabilityContractPath:"contracts.recommendedConsumptionSummaryDiscoverability",
      summarySurfaceGuidePath:"summarySurfaceGuide.recommendedConsumption",
      summarySurfaceGuideContractPath:"contracts.summarySurfaceGuide",
      recommendedLookupOrder:["metaDiscoverabilityContractPath","metaPath","metaContractPath","currentInstancePath","currentContractPath","summaryMetaPath","summaryDiscoverabilityPath"]
    }'
}

osc_operator_signal_recommended_consumption_paths_json() {
  jq -cn '{
    contracts:{
      sharedCore:"contracts.recommendedConsumption.sharedCore",
      repoRootInstall:"contracts.recommendedConsumption.repoRootInstall",
      catalogReader:"contracts.recommendedConsumption.catalogReader",
      powershellRegister:"contracts.recommendedConsumption.powershellRegister"
    },
    instances:{
      repoRootInstall:"cli.repoRootInstall.recommendedConsumption",
      catalogReader:"cli.catalogReader.recommendedConsumption",
      powershellRegister:"cli.powershellRegister.recommendedConsumption"
    },
    meta:{
      repoRootInstall:"cli.repoRootInstall.recommendedConsumptionMeta",
      catalogReader:"cli.catalogReader.recommendedConsumptionMeta",
      powershellRegister:"cli.powershellRegister.recommendedConsumptionMeta"
    },
    metaDiscoverability:{
      repoRootInstall:"cli.repoRootInstall.recommendedConsumptionMeta.metaDiscoverability",
      catalogReader:"cli.catalogReader.recommendedConsumptionMeta.metaDiscoverability",
      powershellRegister:"cli.powershellRegister.recommendedConsumptionMeta.metaDiscoverability"
    },
    summaryMeta:{
      repoRootInstall:"cli.repoRootInstall.recommendedConsumption.summaryMeta",
      catalogReader:"cli.catalogReader.recommendedConsumption.summaryMeta",
      powershellRegister:"cli.powershellRegister.recommendedConsumption.summaryMeta"
    },
    summaryDiscoverability:{
      repoRootInstall:"cli.repoRootInstall.recommendedConsumption.summaryDiscoverability",
      catalogReader:"cli.catalogReader.recommendedConsumption.summaryDiscoverability",
      powershellRegister:"cli.powershellRegister.recommendedConsumption.summaryDiscoverability"
    },
    summaryContracts:{
      summary:"contracts.recommendedConsumptionSummary",
      discoverability:"contracts.recommendedConsumptionSummaryDiscoverability"
    },
    topLevelSummary:{
      discoverability:"summaryDiscoverability.recommendedConsumption",
      surfaceGuide:"summarySurfaceGuide.recommendedConsumption"
    },
    topLevelSummaryContracts:{
      discoverability:"contracts.summaryDiscoverability",
      surfaceGuide:"contracts.summarySurfaceGuide"
    },
    powershellMirrorGuidance:{
      start:"cli.powershellRegister.recommendedConsumption.metaDiscoverabilityStartPath",
      readOrder:"cli.powershellRegister.recommendedConsumption.recommendedDocsMirrorReadOrder",
      commands:"cli.powershellRegister.recommendedConsumption.discoveryCommands",
      coverage:"coverage.powershellRegister",
      notes:"cli.powershellRegister.notes",
      instance:"cli.powershellRegister.recommendedConsumption"
    }
  }'
}

osc_operator_signal_summary_discoverability_json() {
  local recommended_consumption_paths_json
  recommended_consumption_paths_json="$(osc_operator_signal_recommended_consumption_paths_json)"

  jq -cn \
    --argjson recommendedConsumptionPaths "$recommended_consumption_paths_json" \
    '{
      contractPath:"contracts.summaryDiscoverability",
      recommendedConsumption:{
        summary:"Top-level entry point for the recommendedConsumption short-summary layer across repo-root, catalog-reader, and documented PowerShell mirror surfaces.",
        preferredStartPath:"summaryDiscoverability.recommendedConsumption",
        preferredEntryPath:$recommendedConsumptionPaths.summaryDiscoverability.repoRootInstall,
        primaryField:"summaryDisplay",
        compactField:"compactSummary",
        instancePaths:$recommendedConsumptionPaths.instances,
        summaryMetaPaths:$recommendedConsumptionPaths.summaryMeta,
        lightEntryPaths:$recommendedConsumptionPaths.summaryDiscoverability,
        summaryContractPaths:$recommendedConsumptionPaths.summaryContracts,
        surfaceGuidePath:"summarySurfaceGuide.recommendedConsumption",
        surfaceGuideContractPath:"contracts.summarySurfaceGuide",
        schemaCommands:{repoRoot:"./install-operator-signal-cron.sh --schema",catalogReader:"./scripts/read-operator-signal-cron-catalog.sh --schema"},
        preferredReadOrder:[
          {order:1,target:"entry",path:"summaryDiscoverability.recommendedConsumption",summary:"Start from the top-level entry object when you only need the shortest stable landing zone for the summary layer."},
          {order:2,target:"contract",path:"contracts.summaryDiscoverability",summary:"Validate the top-level entry block against its focused contract before relying on deeper summary-layer fields."},
          {order:3,target:"surfaceGuide",path:"summarySurfaceGuide.recommendedConsumption",summary:"Follow the surface guide when you need a stable map of which help, schema, and compact surfaces mirror this summary layer."},
          {order:4,target:"summaryMeta",path:"cli.repoRootInstall.recommendedConsumption.summaryMeta",summary:"Expand into summary metadata only when you need the fuller shared short-summary layer beyond the top-level entry."}
        ],
        notes:[
          "Start here when you only need the short summary layer and do not want to inspect the full recommendedConsumption object first.",
          "Use summaryContractPaths.discoverability to validate the light-entry block, then follow summaryMetaPaths when you need the fuller summary layer.",
          "Use surfaceGuidePath when you need a stable map of which lightweight and schema surfaces mirror this top-level summary layer."
        ]
      }
    }'
}

osc_operator_signal_summary_discoverability_contract_json() {
  jq -cn '{
    fields:{contractPath:"string",recommendedConsumption:"object"},
    recommendedConsumptionFields:{summary:"string",preferredStartPath:"string",preferredEntryPath:"string",primaryField:"string",compactField:"string",instancePaths:"object",summaryMetaPaths:"object",lightEntryPaths:"object",summaryContractPaths:"object",surfaceGuidePath:"string",surfaceGuideContractPath:"string",schemaCommands:"object",preferredReadOrder:"object[]",notes:"string[]"},
    instancePathKeys:{repoRootInstall:"string",catalogReader:"string",powershellRegister:"string"},
    summaryMetaPathKeys:{repoRootInstall:"string",catalogReader:"string",powershellRegister:"string"},
    lightEntryPathKeys:{repoRootInstall:"string",catalogReader:"string",powershellRegister:"string"},
    summaryContractKeys:{summary:"string",discoverability:"string"},
    schemaCommandKeys:{repoRoot:"string",catalogReader:"string"},
    preferredReadOrderFields:{order:"number",target:"string",path:"string",summary:"string"}
  }'
}

osc_operator_signal_summary_surface_guide_json() {
  jq -cn \
    --arg helpKind "$OSC_INSTALL_HELP_KIND" \
    --argjson helpSchemaVersion "$OSC_INSTALL_HELP_SCHEMA_VERSION" \
    --arg doctorKind "$OSC_INSTALL_DOCTOR_KIND" \
    --argjson doctorSchemaVersion "$OSC_INSTALL_DOCTOR_SCHEMA_VERSION" \
    --arg schemaKind "$OSC_INSTALL_SCHEMA_KIND" \
    --argjson schemaSurfaceVersion "$OSC_INSTALL_SCHEMA_VERSION" \
    --arg catalogKind "$OSC_OPERATOR_SIGNAL_CATALOG_KIND" \
    --argjson catalogSchemaVersion "$OSC_OPERATOR_SIGNAL_CATALOG_SCHEMA_VERSION" \
    --arg catalogReaderHelpKind "$OSC_OPERATOR_SIGNAL_CATALOG_READER_HELP_KIND" \
    --argjson catalogReaderHelpSchemaVersion "$OSC_OPERATOR_SIGNAL_CATALOG_READER_HELP_SCHEMA_VERSION" \
    --arg catalogReaderSchemaKind "$OSC_OPERATOR_SIGNAL_CATALOG_READER_SCHEMA_KIND" \
    --argjson catalogReaderSchemaVersion "$OSC_OPERATOR_SIGNAL_CATALOG_READER_SCHEMA_VERSION" \
    '{
      contractPath:"contracts.summarySurfaceGuide",
      recommendedConsumption:{
        summary:"Stable guide to the lightweight and schema surfaces that mirror the top-level recommendedConsumption summary layer.",
        preferredStartPath:"summarySurfaceGuide.recommendedConsumption",
        preferredCompactSurface:{command:"./scripts/read-operator-signal-cron-catalog.sh --compact",path:"summaryDiscoverability.recommendedConsumption",kind:$catalogKind,schemaVersion:$catalogSchemaVersion},
        preferredSchemaSurface:{command:"./install-operator-signal-cron.sh --schema",path:"related.catalogJson.summaryDiscoverability",kind:$schemaKind,schemaVersion:$schemaSurfaceVersion},
        surfaces:{
          repoRootHelpJson:{command:"./install-operator-signal-cron.sh --help-json",path:"catalogSummaryDiscoverability",kind:$helpKind,schemaVersion:$helpSchemaVersion},
          repoRootDoctorJson:{command:"./install-operator-signal-cron.sh --doctor-json",path:"catalogSummaryDiscoverability",kind:$doctorKind,schemaVersion:$doctorSchemaVersion},
          repoRootSchema:{command:"./install-operator-signal-cron.sh --schema",path:"related.catalogJson.summaryDiscoverability",kind:$schemaKind,schemaVersion:$schemaSurfaceVersion},
          catalogReaderHelpJson:{command:"./scripts/read-operator-signal-cron-catalog.sh --help-json",path:"catalogSummaryDiscoverability",kind:$catalogReaderHelpKind,schemaVersion:$catalogReaderHelpSchemaVersion},
          catalogReaderSchema:{command:"./scripts/read-operator-signal-cron-catalog.sh --schema",path:"outputs.catalogJson.summaryDiscoverability",kind:$catalogReaderSchemaKind,schemaVersion:$catalogReaderSchemaVersion},
          catalogJson:{command:"./scripts/read-operator-signal-cron-catalog.sh --compact",path:"summaryDiscoverability",kind:$catalogKind,schemaVersion:$catalogSchemaVersion}
        },
        recommendedReadOrder:[
          {order:1,surface:"catalogJson",path:"summaryDiscoverability.recommendedConsumption",summary:"Read the top-level entry object first when you want the shortest stable summary-layer landing zone."},
          {order:2,surface:"repoRootSchema",path:"related.catalogJson.summaryDiscoverability",summary:"Use the repo-root schema surface when you need the summary-layer path advertised from the main entrypoint."},
          {order:3,surface:"catalogReaderSchema",path:"outputs.catalogJson.summaryDiscoverability",summary:"Use the catalog-reader schema when you want the summary-layer schema mirror plus related output contracts."},
          {order:4,surface:"repoRootHelpJson",path:"catalogSummaryDiscoverability",summary:"Use lightweight help/doctor surfaces for quick machine-readable discovery without expanding the full schema."}
        ],
        notes:[
          "Prefer preferredCompactSurface for direct machine-readable summary discovery.",
          "Prefer preferredSchemaSurface when a schema-first consumer wants the main repo-root entrypoint to advertise the summary layer.",
          "Help and doctor JSON surfaces mirror the same top-level summary entry for lightweight consumers, but they are not the primary long-term contract surface."
        ]
      }
    }'
}

osc_operator_signal_summary_surface_guide_contract_json() {
  jq -cn '{
    fields:{contractPath:"string",recommendedConsumption:"object"},
    recommendedConsumptionFields:{summary:"string",preferredStartPath:"string",preferredCompactSurface:"object",preferredSchemaSurface:"object",surfaces:"object",recommendedReadOrder:"object[]",notes:"string[]"},
    surfaceFields:{command:"string",path:"string",kind:"string",schemaVersion:"number"},
    surfaceKeys:{repoRootHelpJson:"object",repoRootDoctorJson:"object",repoRootSchema:"object",catalogReaderHelpJson:"object",catalogReaderSchema:"object",catalogJson:"object"},
    recommendedReadOrderFields:{order:"number",surface:"string",path:"string",summary:"string"}
  }'
}

osc_operator_signal_print_catalog_reader_usage() {
  cat <<EOF
Usage: ${OSC_OPERATOR_SIGNAL_CATALOG_READER_USAGE}

${OSC_OPERATOR_SIGNAL_CATALOG_READER_SUMMARY}

Options:
  --compact             Print compact single-line JSON
  --field <dotted.path> Print only a catalog subfield, for example:
                        defaults.policyProfile
                        supported.policyBundles
                        cli.register.examples
  --help-json           Print a machine-readable help surface for this catalog reader
  --schema              Print the schema/contract for this catalog reader
  -h, --help            Show this help

Recommended consumption:
$(osc_operator_signal_print_recommended_consumption_lines "$(osc_catalog_reader_recommended_consumption_json)")

Per-CLI self-description layer:
$(osc_operator_signal_print_meta_layer_lines "catalogReader")

Top-level summary layer:
$(osc_operator_signal_print_summary_layer_lines)

Examples:
$(osc_operator_signal_catalog_reader_examples_json | jq -r '.[] | "  " + .')
EOF
}

osc_operator_signal_print_catalog_reader_help_json() {
  local examples_json recommended_consumption_json recommended_consumption_meta_contract_json recommended_consumption_meta_discoverability_json summary_discoverability_json summary_discoverability_contract_json summary_surface_guide_json summary_surface_guide_contract_json
  examples_json="$(osc_operator_signal_catalog_reader_examples_json)"
  recommended_consumption_json="$(osc_catalog_reader_recommended_consumption_json)"
  recommended_consumption_meta_contract_json="$(osc_operator_signal_recommended_consumption_meta_contract_json)"
  recommended_consumption_meta_discoverability_json="$(osc_operator_signal_recommended_consumption_meta_discoverability_json "catalogReader")"
  summary_discoverability_json="$(osc_operator_signal_summary_discoverability_json)"
  summary_discoverability_contract_json="$(osc_operator_signal_summary_discoverability_contract_json)"
  summary_surface_guide_json="$(osc_operator_signal_summary_surface_guide_json)"
  summary_surface_guide_contract_json="$(osc_operator_signal_summary_surface_guide_contract_json)"

  jq -cn \
    --arg kind "$OSC_OPERATOR_SIGNAL_CATALOG_READER_HELP_KIND" \
    --argjson schemaVersion "$OSC_OPERATOR_SIGNAL_CATALOG_READER_HELP_SCHEMA_VERSION" \
    --arg usage "$OSC_OPERATOR_SIGNAL_CATALOG_READER_USAGE" \
    --arg summary "$OSC_OPERATOR_SIGNAL_CATALOG_READER_SUMMARY" \
    --arg outputKind "$OSC_OPERATOR_SIGNAL_CATALOG_KIND" \
    --argjson outputSchemaVersion "$OSC_OPERATOR_SIGNAL_CATALOG_SCHEMA_VERSION" \
    --argjson examples "$examples_json" \
    --argjson recommendedConsumption "$recommended_consumption_json" \
    --argjson recommendedConsumptionMetaContract "$recommended_consumption_meta_contract_json" \
    --argjson recommendedConsumptionMetaDiscoverability "$recommended_consumption_meta_discoverability_json" \
    --argjson summaryDiscoverability "$summary_discoverability_json" \
    --argjson summaryDiscoverabilityContract "$summary_discoverability_contract_json" \
    --argjson summarySurfaceGuide "$summary_surface_guide_json" \
    --argjson summarySurfaceGuideContract "$summary_surface_guide_contract_json" \
    '{kind:$kind,schemaVersion:$schemaVersion,usage:$usage,summary:$summary,outputs:{catalogJson:{kind:$outputKind,schemaVersion:$outputSchemaVersion}},flags:{compact:"boolean",field:"string",helpJson:"boolean",schema:"boolean"},examples:$examples,recommendedConsumption:$recommendedConsumption,recommendedConsumptionMetaPath:"cli.catalogReader.recommendedConsumptionMeta",recommendedConsumptionMetaContractPath:"contracts.recommendedConsumptionMeta",recommendedConsumptionMetaContract:$recommendedConsumptionMetaContract,recommendedConsumptionMetaDiscoverabilityPath:"cli.catalogReader.recommendedConsumptionMeta.metaDiscoverability",recommendedConsumptionMetaDiscoverabilityContractPath:"contracts.recommendedConsumptionMetaDiscoverability",recommendedConsumptionMetaDiscoverability:$recommendedConsumptionMetaDiscoverability,catalogSummaryDiscoverabilityPath:"summaryDiscoverability.recommendedConsumption",catalogSummaryDiscoverabilityContractPath:"contracts.summaryDiscoverability",catalogSummaryDiscoverability:$summaryDiscoverability,catalogSummaryDiscoverabilityContract:$summaryDiscoverabilityContract,catalogSummarySurfaceGuidePath:"summarySurfaceGuide.recommendedConsumption",catalogSummarySurfaceGuideContractPath:"contracts.summarySurfaceGuide",catalogSummarySurfaceGuide:$summarySurfaceGuide,catalogSummarySurfaceGuideContract:$summarySurfaceGuideContract}'
}

osc_operator_signal_print_catalog_reader_schema_json() {
  local recommended_consumption_contract_json recommended_consumption_meta_contract_json recommended_consumption_meta_discoverability_json recommended_consumption_meta_discoverability_contract_json recommended_consumption_paths_json recommended_consumption_summary_contract_json recommended_consumption_summary_discoverability_contract_json summary_discoverability_contract_json summary_surface_guide_contract_json
  recommended_consumption_contract_json="$(osc_operator_signal_recommended_consumption_contract_json)"
  recommended_consumption_meta_contract_json="$(osc_operator_signal_recommended_consumption_meta_contract_json)"
  recommended_consumption_meta_discoverability_json="$(osc_operator_signal_recommended_consumption_meta_discoverability_json "catalogReader")"
  recommended_consumption_meta_discoverability_contract_json="$(osc_operator_signal_recommended_consumption_meta_discoverability_contract_json)"
  recommended_consumption_paths_json="$(osc_operator_signal_recommended_consumption_paths_json)"
  recommended_consumption_summary_contract_json="$(osc_operator_signal_recommended_consumption_summary_contract_json)"
  recommended_consumption_summary_discoverability_contract_json="$(osc_operator_signal_recommended_consumption_summary_discoverability_contract_json)"
  summary_discoverability_contract_json="$(osc_operator_signal_summary_discoverability_contract_json)"
  summary_surface_guide_contract_json="$(osc_operator_signal_summary_surface_guide_contract_json)"

  jq -cn \
    --arg kind "$OSC_OPERATOR_SIGNAL_CATALOG_READER_SCHEMA_KIND" \
    --argjson schemaVersion "$OSC_OPERATOR_SIGNAL_CATALOG_READER_SCHEMA_VERSION" \
    --arg usage "$OSC_OPERATOR_SIGNAL_CATALOG_READER_USAGE" \
    --arg summary "$OSC_OPERATOR_SIGNAL_CATALOG_READER_SUMMARY" \
    --arg outputKind "$OSC_OPERATOR_SIGNAL_CATALOG_KIND" \
    --argjson outputSchemaVersion "$OSC_OPERATOR_SIGNAL_CATALOG_SCHEMA_VERSION" \
    --arg helpKind "$OSC_OPERATOR_SIGNAL_CATALOG_READER_HELP_KIND" \
    --argjson helpSchemaVersion "$OSC_OPERATOR_SIGNAL_CATALOG_READER_HELP_SCHEMA_VERSION" \
    --argjson recommendedConsumptionContract "$recommended_consumption_contract_json" \
    --argjson recommendedConsumptionMetaContract "$recommended_consumption_meta_contract_json" \
    --argjson recommendedConsumptionMetaDiscoverability "$recommended_consumption_meta_discoverability_json" \
    --argjson recommendedConsumptionMetaDiscoverabilityContract "$recommended_consumption_meta_discoverability_contract_json" \
    --argjson recommendedConsumptionPaths "$recommended_consumption_paths_json" \
    --argjson recommendedConsumptionSummaryContract "$recommended_consumption_summary_contract_json" \
    --argjson recommendedConsumptionSummaryDiscoverabilityContract "$recommended_consumption_summary_discoverability_contract_json" \
    --argjson summaryDiscoverability "$(osc_operator_signal_summary_discoverability_json)" \
    --argjson summaryDiscoverabilityContract "$summary_discoverability_contract_json" \
    --argjson summarySurfaceGuide "$(osc_operator_signal_summary_surface_guide_json)" \
    --argjson summarySurfaceGuideContract "$summary_surface_guide_contract_json" \
    '{kind:$kind,schemaVersion:$schemaVersion,usage:$usage,summary:$summary,flags:{compact:"boolean",field:"string",helpJson:"boolean",schema:"boolean"},outputs:{helpJson:{kind:$helpKind,schemaVersion:$helpSchemaVersion,summary:"Machine-readable help surface for this catalog reader.",fields:{kind:"string",schemaVersion:"number",usage:"string",summary:"string",outputs:"object",flags:"object",examples:"string[]",recommendedConsumption:"object",recommendedConsumptionMetaPath:"string",recommendedConsumptionMetaContractPath:"string",recommendedConsumptionMetaContract:"object",recommendedConsumptionMetaDiscoverabilityPath:"string",recommendedConsumptionMetaDiscoverabilityContractPath:"string",recommendedConsumptionMetaDiscoverability:"object",catalogSummaryDiscoverabilityPath:"string",catalogSummaryDiscoverabilityContractPath:"string",catalogSummaryDiscoverability:"object",catalogSummaryDiscoverabilityContract:"object",catalogSummarySurfaceGuidePath:"string",catalogSummarySurfaceGuideContractPath:"string",catalogSummarySurfaceGuide:"object",catalogSummarySurfaceGuideContract:"object"}},catalogJson:{kind:$outputKind,schemaVersion:$outputSchemaVersion,summary:"Shared machine-readable catalog for the repo-root alias, script install helper, register helper, catalog reader, and documented PowerShell helper.",fields:{kind:"string",schemaVersion:"number",repoRoot:"string",defaults:"object",supported:"object",provenance:"object",coverage:"object",evolution:"object",capabilities:"object",surfaces:"object",contracts:"object",summaryDiscoverabilityPath:"string",summaryDiscoverabilityContractPath:"string",summaryDiscoverabilityContract:"object",summaryDiscoverability:"object",summarySurfaceGuidePath:"string",summarySurfaceGuideContractPath:"string",summarySurfaceGuideContract:"object",summarySurfaceGuide:"object",cli:"object"},summaryDiscoverabilityPath:"summaryDiscoverability.recommendedConsumption",summaryDiscoverabilityContractPath:"contracts.summaryDiscoverability",summaryDiscoverability:$summaryDiscoverability,summaryDiscoverabilityContract:$summaryDiscoverabilityContract,summarySurfaceGuidePath:"summarySurfaceGuide.recommendedConsumption",summarySurfaceGuideContractPath:"contracts.summarySurfaceGuide",summarySurfaceGuide:$summarySurfaceGuide,summarySurfaceGuideContract:$summarySurfaceGuideContract,recommendedConsumption:{contractPaths:$recommendedConsumptionPaths,metaPaths:$recommendedConsumptionPaths.meta,metaDiscoverabilityPaths:$recommendedConsumptionPaths.metaDiscoverability,summaryMetaPaths:$recommendedConsumptionPaths.summaryMeta,summaryDiscoverabilityPaths:$recommendedConsumptionPaths.summaryDiscoverability,summaryContractPaths:$recommendedConsumptionPaths.summaryContracts,topLevelSummaryPaths:$recommendedConsumptionPaths.topLevelSummary,topLevelSummaryContractPaths:$recommendedConsumptionPaths.topLevelSummaryContracts,powershellMirrorGuidancePaths:$recommendedConsumptionPaths.powershellMirrorGuidance,powershellMirrorConsumerExampleId:"powershell-mirror-guided-read",metaContractPath:"contracts.recommendedConsumptionMeta",metaDiscoverabilityContractPath:"contracts.recommendedConsumptionMetaDiscoverability",summaryContractPath:"contracts.recommendedConsumptionSummary",summaryDiscoverabilityContractPath:"contracts.recommendedConsumptionSummaryDiscoverability",summarySurfaceGuideContractPath:"contracts.summarySurfaceGuide",contract:$recommendedConsumptionContract,metaContract:$recommendedConsumptionMetaContract,metaDiscoverability:$recommendedConsumptionMetaDiscoverability,metaDiscoverabilityContract:$recommendedConsumptionMetaDiscoverabilityContract,summaryContract:$recommendedConsumptionSummaryContract,summaryDiscoverabilityContract:$recommendedConsumptionSummaryDiscoverabilityContract}},fieldValue:{summary:"Single subfield extracted from catalogJson via dotted jq path.",type:"json"}}}'
}
