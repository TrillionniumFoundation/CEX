#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
RELOAD_HELPER="$SCRIPT_DIR/reload-session-auth-runtime.sh"
ACTIVATE_HELPER="$SCRIPT_DIR/activate-session-auth-runtime.sh"
ROLLBACK_HELPER="$SCRIPT_DIR/rollback-session-auth-runtime.sh"
HISTORY_HELPER="$SCRIPT_DIR/read-session-auth-runtime-history.sh"
STATUS_HELPER="$SCRIPT_DIR/read-session-auth-runtime-activation-status.sh"

run_capture_json() {
  local name="$1"
  shift
  local stdout_file stderr_file exit_code
  stdout_file=$(mktemp)
  stderr_file=$(mktemp)
  exit_code=0
  set +e
  "$@" >"$stdout_file" 2>"$stderr_file"
  exit_code=$?
  set -e
  python3 - "$name" "$exit_code" "$stdout_file" "$stderr_file" <<'PY'
from __future__ import annotations
import json, pathlib, sys
name, exit_code_raw, stdout_path, stderr_path = sys.argv[1:5]
stdout_text = pathlib.Path(stdout_path).read_text(encoding='utf-8')
stderr_text = pathlib.Path(stderr_path).read_text(encoding='utf-8')
try:
    exit_code = int(exit_code_raw)
except Exception:
    exit_code = 1
parsed = None
if stdout_text.strip():
    try:
        parsed = json.loads(stdout_text)
    except Exception:
        parsed = None
payload = {
    'name': name,
    'ok': exit_code == 0,
    'exitCode': exit_code,
    'output': parsed,
    'outputText': stdout_text.strip() or None,
    'stderrText': stderr_text.strip() or None,
}
print(json.dumps(payload, ensure_ascii=False))
PY
  rm -f "$stdout_file" "$stderr_file"
}

capture_output_field() {
  local capture_json="$1"
  local dotted_path="$2"
  python3 - "$capture_json" "$dotted_path" <<'PY'
from __future__ import annotations
import json, sys
capture = json.loads(sys.argv[1])
dotted_path = sys.argv[2]
current = capture.get('output')
for part in dotted_path.split('.'):
    if isinstance(current, dict) and part in current:
        current = current[part]
    else:
        current = None
        break
if current is None:
    print('')
elif isinstance(current, (dict, list)):
    print(json.dumps(current, ensure_ascii=False))
else:
    print(current)
PY
}

collect_status_json() {
  local activation_defaults rollback_defaults summary_file history_file live_status last_summary history_summary
  activation_defaults=$(run_capture_json activationDefaults "$ACTIVATE_HELPER" --print-defaults --compact)
  rollback_defaults=$(run_capture_json rollbackDefaults "$ROLLBACK_HELPER" --print-defaults --compact)
  summary_file="$(capture_output_field "$activation_defaults" summaryFile)"
  history_file="$(capture_output_field "$activation_defaults" historyFile)"
  live_status=$(run_capture_json liveStatus "$RELOAD_HELPER" --action status --service both --compact)
  if [[ -n "$summary_file" ]]; then
    last_summary=$(run_capture_json lastSummary "$STATUS_HELPER" --summary-file "$summary_file" --json)
  else
    last_summary=$(python3 - <<'PY'
import json
print(json.dumps({'name':'lastSummary','ok':False,'exitCode':1,'output':None,'outputText':None,'stderrText':'no summaryFile resolved from activation defaults'}))
PY
)
  fi
  if [[ -n "$history_file" ]]; then
    history_summary=$(run_capture_json historySummary "$HISTORY_HELPER" --history-file "$history_file" --summary --json)
  else
    history_summary=$(python3 - <<'PY'
import json
print(json.dumps({'name':'historySummary','ok':False,'exitCode':1,'output':None,'outputText':None,'stderrText':'no historyFile resolved from activation defaults'}))
PY
)
  fi
  python3 - "$activation_defaults" "$rollback_defaults" "$live_status" "$last_summary" "$history_summary" <<'PY'
from __future__ import annotations
import json, sys
activation_defaults = json.loads(sys.argv[1])
rollback_defaults = json.loads(sys.argv[2])
live_status = json.loads(sys.argv[3])
last_summary = json.loads(sys.argv[4])
history_summary = json.loads(sys.argv[5])

live_output = live_status.get('output') or {}
last_output = last_summary.get('output') or {}
history_output = history_summary.get('output') or {}

errors = []
if not live_status.get('ok'):
    errors.append('live_status_unavailable')
if not activation_defaults.get('ok'):
    errors.append('activation_defaults_unavailable')
if not rollback_defaults.get('ok'):
    errors.append('rollback_defaults_unavailable')
if not last_summary.get('ok'):
    errors.append('last_summary_unavailable')
if not history_summary.get('ok'):
    errors.append('history_summary_unavailable')

status = 'ok'
if errors:
    status = 'warn'
if not live_status.get('ok') or not activation_defaults.get('ok') or not rollback_defaults.get('ok'):
    status = 'error'

requires_attention = bool(errors)
summary_display = (
    f"status:{status}|live:{live_status.get('ok')}|last:{last_summary.get('ok')}|history:{history_summary.get('ok')}|"
    f"lastStatus:{((last_output.get('overall') or {}).get('status'))}|historyStatus:{((history_output.get('overall') or {}).get('status'))}"
)

payload = {
    'kind': 'session-auth-runtime-status',
    'schemaVersion': 1,
    'defaults': {
        'activation': activation_defaults,
        'rollback': rollback_defaults,
    },
    'liveStatus': live_status,
    'lastSummary': last_summary,
    'historySummary': history_summary,
    'overall': {
        'status': status,
        'requiresAttention': requires_attention,
        'errors': errors,
        'summaryDisplay': summary_display,
        'liveStatusOk': live_status.get('ok'),
        'lastSummaryOk': last_summary.get('ok'),
        'historySummaryOk': history_summary.get('ok'),
        'liveRevisionMatch': live_output.get('postStatus', {}).get('liveRevisionMatch') if isinstance(live_output, dict) else None,
        'lastKnownStatus': (last_output.get('overall') or {}).get('status') if isinstance(last_output, dict) else None,
        'historyLatestStatus': (history_output.get('overall') or {}).get('status') if isinstance(history_output, dict) else None,
    },
}
print(json.dumps(payload, ensure_ascii=False, indent=2))
PY
}

collect_doctor_json() {
  local status_json
  status_json="$(collect_status_json)"
  python3 - "$status_json" "$RELOAD_HELPER" "$ACTIVATE_HELPER" "$ROLLBACK_HELPER" "$HISTORY_HELPER" "$STATUS_HELPER" <<'PY'
from __future__ import annotations
import json, os, sys
status = json.loads(sys.argv[1])
helper_paths = {
    'reloadHelper': sys.argv[2],
    'activateHelper': sys.argv[3],
    'rollbackHelper': sys.argv[4],
    'historyHelper': sys.argv[5],
    'lastHelper': sys.argv[6],
}
helper_checks = {
    name: {
        'path': path,
        'exists': os.path.exists(path),
        'executable': os.path.isfile(path) and os.access(path, os.X_OK),
    }
    for name, path in helper_paths.items()
}
all_executable = all(item['executable'] for item in helper_checks.values())
defaults = status.get('defaults') or {}
activation_defaults = (defaults.get('activation') or {}).get('output') or {}
rollback_defaults = (defaults.get('rollback') or {}).get('output') or {}
checks = {
    'helpersExecutable': all_executable,
    'activationDefaultsReadable': (defaults.get('activation') or {}).get('ok') is True,
    'rollbackDefaultsReadable': (defaults.get('rollback') or {}).get('ok') is True,
    'liveStatusReachable': ((status.get('liveStatus') or {}).get('ok') is True),
    'lastSummaryReadable': ((status.get('lastSummary') or {}).get('ok') is True),
    'historySummaryReadable': ((status.get('historySummary') or {}).get('ok') is True),
}
doctor_status = status.get('overall', {}).get('status') or 'warn'
if not all_executable:
    doctor_status = 'error'
requires_attention = (doctor_status != 'ok') or not all_executable
payload = {
    'kind': 'session-auth-runtime-doctor',
    'schemaVersion': 1,
    'helpers': helper_checks,
    'defaults': {
        'activation': activation_defaults,
        'rollback': rollback_defaults,
    },
    'status': status,
    'checks': checks,
    'overall': {
        'status': doctor_status,
        'requiresAttention': requires_attention,
        'summaryDisplay': f"doctor:{doctor_status}|helpers:{all_executable}|live:{checks['liveStatusReachable']}|last:{checks['lastSummaryReadable']}|history:{checks['historySummaryReadable']}",
    },
}
print(json.dumps(payload, ensure_ascii=False, indent=2))
PY
}

collect_summary_json() {
  python3 - <<PY
import json
commands = ["status", "validate", "reload", "activate", "rollback", "history", "last"]
surfaces = [
  "help", "examples", "printRunCommand", "helpJson", "schema",
  "summaryJson", "summaryCompact", "summaryField",
  "statusJson", "statusCompact", "statusField",
  "doctor", "doctorJson", "doctorCompact", "doctorField"
]
recommended_run_command = "./scripts/session-auth-runtime.sh --status-compact"
first_commands = [
  "./scripts/session-auth-runtime.sh --status-compact",
  "./scripts/session-auth-runtime.sh --doctor-compact",
  "./scripts/session-auth-runtime.sh history --summary --compact"
]
payload = {
  "kind": "session-auth-runtime-summary",
  "schemaVersion": 1,
  "name": "session-auth-runtime",
  "usage": "./scripts/session-auth-runtime.sh <command> [args...]",
  "commands": commands,
  "surfaces": surfaces,
  "summarySurfaceGuide": {
    "summary": {
      "json": "./scripts/session-auth-runtime.sh --summary-json",
      "compact": "./scripts/session-auth-runtime.sh --summary-compact",
      "fieldExample": "./scripts/session-auth-runtime.sh --summary-field overall.surfaceCount",
    },
    "status": {
      "json": "./scripts/session-auth-runtime.sh --status-json",
      "compact": "./scripts/session-auth-runtime.sh --status-compact",
      "fieldExample": "./scripts/session-auth-runtime.sh --status-field overall.lastKnownStatus",
    },
    "doctor": {
      "text": "./scripts/session-auth-runtime.sh --doctor",
      "json": "./scripts/session-auth-runtime.sh --doctor-json",
      "compact": "./scripts/session-auth-runtime.sh --doctor-compact",
      "fieldExample": "./scripts/session-auth-runtime.sh --doctor-field overall.status",
    },
    "discoverability": {
      "helpJson": "./scripts/session-auth-runtime.sh --help-json",
      "schema": "./scripts/session-auth-runtime.sh --schema",
      "examples": "./scripts/session-auth-runtime.sh --examples",
      "printRunCommand": "./scripts/session-auth-runtime.sh --print-run-command",
    },
  },
  "metaDiscoverability": {
    "summarySurfaceGuidePath": "summarySurfaceGuide",
    "summarySurfaceGuideContractPath": "contracts.summarySurfaceGuide",
    "recommendedConsumptionPath": "recommendedConsumption",
    "recommendedConsumptionContractPath": "contracts.recommendedConsumption",
    "catalogEntryPath": "catalogEntry",
    "catalogEntryContractPath": "contracts.catalogEntry",
    "surfaceCapabilitiesPath": "surfaceCapabilities",
    "surfaceCapabilitiesContractPath": "contracts.surfaceCapabilities",
    "stabilityPolicyPath": "stabilityPolicy",
    "stabilityPolicyContractPath": "contracts.stabilityPolicy",
    "surfaceProfilesPath": "surfaceProfiles",
    "surfaceProfilesContractPath": "contracts.surfaceProfiles",
    "consumerProfilesPath": "consumerProfiles",
    "consumerProfilesContractPath": "contracts.consumerProfiles",
    "profileSelectionGuidePath": "profileSelectionGuide",
    "profileSelectionGuideContractPath": "contracts.profileSelectionGuide",
    "profileSelectionTracePath": "profileSelectionTrace",
    "profileSelectionTraceContractPath": "contracts.profileSelectionTrace",
    "lifecyclePath": "lifecycle",
    "lifecycleContractPath": "contracts.lifecycle",
    "maturityPath": "maturity",
    "maturityContractPath": "contracts.maturity",
    "compatibilityPolicyPath": "compatibilityPolicy",
    "compatibilityPolicyContractPath": "contracts.compatibilityPolicy",
    "contractGovernancePath": "contractGovernance",
    "contractGovernanceContractPath": "contracts.contractGovernance",
    "surfaceLifecycleMatrixPath": "surfaceLifecycleMatrix",
    "surfaceLifecycleMatrixContractPath": "contracts.surfaceLifecycleMatrix",
    "contractStatusMatrixPath": "contractStatusMatrix",
    "contractStatusMatrixContractPath": "contracts.contractStatusMatrix",
    "helpJsonCommand": "./scripts/session-auth-runtime.sh --help-json",
    "schemaCommand": "./scripts/session-auth-runtime.sh --schema",
    "summaryJsonCommand": "./scripts/session-auth-runtime.sh --summary-json",
  },
  "catalogEntry": {
    "kind": "session-auth-runtime-catalog-entry",
    "schemaVersion": 1,
    "entryId": "session-auth-runtime",
    "primaryCommand": "./scripts/session-auth-runtime.sh --summary-json",
    "preferredReadOrder": [
      "metaDiscoverability",
      "catalogEntry",
      "lifecycle",
      "maturity",
      "compatibilityPolicy",
      "contractGovernance",
      "surfaceLifecycleMatrix",
      "contractStatusMatrix",
      "surfaceCapabilities",
      "stabilityPolicy",
      "summarySurfaceGuide",
      "recommendedConsumption",
      "statusJson",
      "doctorJson",
      "helpJson",
      "schema"
    ],
    "surfaceKinds": {
      "helpJson": "session-auth-runtime-help",
      "summaryJson": "session-auth-runtime-summary",
      "statusJson": "session-auth-runtime-status",
      "doctorJson": "session-auth-runtime-doctor",
      "schema": "session-auth-runtime-schema"
    }
  },
  "surfaceCapabilities": {
    "helpJson": {"machineReadable": True, "stable": True, "glance": False, "compact": False, "field": False},
    "summaryJson": {"machineReadable": True, "stable": True, "glance": False, "compact": False, "field": False},
    "summaryCompact": {"machineReadable": False, "stable": True, "glance": True, "compact": True, "field": False},
    "summaryField": {"machineReadable": False, "stable": True, "glance": False, "compact": False, "field": True},
    "statusJson": {"machineReadable": True, "stable": True, "glance": False, "compact": False, "field": False},
    "statusCompact": {"machineReadable": False, "stable": True, "glance": True, "compact": True, "field": False},
    "statusField": {"machineReadable": False, "stable": True, "glance": False, "compact": False, "field": True},
    "doctor": {"machineReadable": False, "stable": False, "glance": True, "compact": False, "field": False},
    "doctorJson": {"machineReadable": True, "stable": True, "glance": False, "compact": False, "field": False},
    "doctorCompact": {"machineReadable": False, "stable": True, "glance": True, "compact": True, "field": False},
    "doctorField": {"machineReadable": False, "stable": True, "glance": False, "compact": False, "field": True},
    "schema": {"machineReadable": True, "stable": True, "glance": False, "compact": False, "field": False}
  },
  "surfaceProfiles": {
    "machine_reader": {
      "preferredSurfaces": ["summaryJson", "statusJson", "doctorJson", "helpJson", "schema"],
      "entrySurface": "summaryJson",
      "notes": ["Prefer JSON surfaces and ignore unknown fields."]
    },
    "shell_glance": {
      "preferredSurfaces": ["summaryCompact", "statusCompact", "doctorCompact"],
      "entrySurface": "summaryCompact",
      "notes": ["Use compact surfaces for one-line shell/operator snapshots."]
    },
    "field_reader": {
      "preferredSurfaces": ["summaryField", "statusField", "doctorField"],
      "entrySurface": "summaryField",
      "notes": ["Use dotted-path field surfaces for guards and scripts."]
    },
    "operator_debug": {
      "preferredSurfaces": ["statusJson", "doctorJson", "doctor", "helpJson", "schema"],
      "entrySurface": "doctorJson",
      "notes": ["Start with doctor/status, then fall back to help/schema for contracts."]
    }
  },
  "consumerProfiles": {
    "machine_parser": {
      "surfaceProfile": "machine_reader",
      "recommendedStart": "summaryJson",
      "fallback": ["helpJson", "schema"]
    },
    "shell_operator": {
      "surfaceProfile": "shell_glance",
      "recommendedStart": "summaryCompact",
      "fallback": ["statusCompact", "doctorCompact"]
    },
    "schema_reader": {
      "surfaceProfile": "machine_reader",
      "recommendedStart": "schema",
      "fallback": ["helpJson", "summaryJson"]
    },
    "operator_debugger": {
      "surfaceProfile": "operator_debug",
      "recommendedStart": "doctorJson",
      "fallback": ["statusJson", "doctor"]
    }
  },
  "profileSelectionGuide": {
    "ifYouNeed": {
      "machineReadableContract": "machine_parser",
      "oneLineShellStatus": "shell_operator",
      "singleFieldGuard": "field_reader",
      "schemaOrContractLookup": "schema_reader",
      "debugOrTriage": "operator_debugger"
    },
    "selectionOrder": [
      "identify consumer intent",
      "pick consumerProfiles entry",
      "resolve referenced surfaceProfiles entry",
      "start at recommendedStart or entrySurface",
      "fall back using fallback/preferredSurfaces"
    ]
  },
  "profileSelectionTrace": {
    "machine_parser": {
      "consumerProfile": "machine_parser",
      "surfaceProfile": "machine_reader",
      "selectedSurface": "summaryJson",
      "why": "stable machine-readable JSON entry surface",
      "fallback": ["helpJson", "schema"]
    },
    "shell_operator": {
      "consumerProfile": "shell_operator",
      "surfaceProfile": "shell_glance",
      "selectedSurface": "summaryCompact",
      "why": "lowest-friction one-line operator snapshot",
      "fallback": ["statusCompact", "doctorCompact"]
    },
    "schema_reader": {
      "consumerProfile": "schema_reader",
      "surfaceProfile": "machine_reader",
      "selectedSurface": "schema",
      "why": "contract-first schema inspection path",
      "fallback": ["helpJson", "summaryJson"]
    },
    "operator_debugger": {
      "consumerProfile": "operator_debugger",
      "surfaceProfile": "operator_debug",
      "selectedSurface": "doctorJson",
      "why": "richer diagnostic payload before raw text fallback",
      "fallback": ["statusJson", "doctor"]
    }
  },
  "lifecycle": {
    "currentPhase": "self-describing-operator-cli",
    "preferredEvolutionMode": "additive-only",
    "guarantees": [
      "Stable JSON contracts evolve additively.",
      "Compact and field surfaces remain shell/operator friendly.",
      "Help/schema/summary stay aligned as the main self-description trio."
    ],
    "preferredUpgradeOrder": [
      "summaryJson",
      "helpJson",
      "schema",
      "statusJson",
      "doctorJson",
      "compact/field surfaces"
    ]
  },
  "maturity": {
    "machineReadable": {
      "level": "stable",
      "surfaces": ["summaryJson", "helpJson", "schema", "statusJson", "doctorJson"]
    },
    "shellGlance": {
      "level": "stable",
      "surfaces": ["summaryCompact", "statusCompact", "doctorCompact"]
    },
    "fieldConsumption": {
      "level": "stable",
      "surfaces": ["summaryField", "statusField", "doctorField"]
    },
    "diagnosticText": {
      "level": "best_effort",
      "surfaces": ["doctor"]
    }
  },
  "compatibilityPolicy": {
    "contractStrategy": "additive-with-announced-deprecation",
    "unknownFieldHandling": "ignore",
    "stableAnchors": [
      "kind",
      "schemaVersion",
      "commands",
      "surfaces",
      "recommendedConsumption",
      "overall"
    ],
    "pathCompatibility": {
      "preferredBehavior": "add-new-paths-before-shifting-read-order",
      "notes": [
        "Prefer additive path introduction over in-place replacement.",
        "Mirror new hints in summary, help, and schema before making them preferred."
      ]
    },
    "deprecation": {
      "mode": "announce-then-remove",
      "minimumBehavior": [
        "add a replacement path or surface first",
        "mirror the replacement in help_json.related and schema contracts",
        "keep deprecated paths during a transition window"
      ],
      "currentlyDeprecated": []
    }
  },
  "contractGovernance": {
    "governedBlocks": [
      "lifecycle",
      "maturity",
      "compatibilityPolicy",
      "stabilityPolicy"
    ],
    "preferredInspectionOrder": [
      "lifecycle",
      "maturity",
      "compatibilityPolicy",
      "stabilityPolicy"
    ],
    "operatorRule": "read-summary-then-contract-governance-before-consuming-lower-level-paths",
    "changeManagement": {
      "requiresMirroringAcross": ["summaryJson", "helpJson", "schema"],
      "requiresSmokeCoverage": True,
      "defaultPolicy": "additive-first"
    }
  },
  "surfaceLifecycleMatrix": {
    "helpJson": {"family": "machineReadable", "status": "stable", "preferred": False, "deprecated": False},
    "summaryJson": {"family": "machineReadable", "status": "stable", "preferred": True, "deprecated": False},
    "summaryCompact": {"family": "shellGlance", "status": "stable", "preferred": True, "deprecated": False},
    "summaryField": {"family": "fieldConsumption", "status": "stable", "preferred": True, "deprecated": False},
    "schema": {"family": "machineReadable", "status": "stable", "preferred": True, "deprecated": False},
    "statusJson": {"family": "machineReadable", "status": "stable", "preferred": True, "deprecated": False},
    "statusCompact": {"family": "shellGlance", "status": "stable", "preferred": True, "deprecated": False},
    "statusField": {"family": "fieldConsumption", "status": "stable", "preferred": True, "deprecated": False},
    "doctor": {"family": "diagnosticText", "status": "best_effort", "preferred": False, "deprecated": False},
    "doctorJson": {"family": "machineReadable", "status": "stable", "preferred": True, "deprecated": False},
    "doctorCompact": {"family": "shellGlance", "status": "stable", "preferred": True, "deprecated": False},
    "doctorField": {"family": "fieldConsumption", "status": "stable", "preferred": True, "deprecated": False}
  },
  "contractStatusMatrix": {
    "summarySurfaceGuide": {"status": "stable", "governedBy": "contractGovernance", "preferred": True},
    "metaDiscoverability": {"status": "stable", "governedBy": "contractGovernance", "preferred": True},
    "recommendedConsumption": {"status": "stable", "governedBy": "contractGovernance", "preferred": True},
    "catalogEntry": {"status": "stable", "governedBy": "contractGovernance", "preferred": True},
    "lifecycle": {"status": "stable", "governedBy": "contractGovernance", "preferred": True},
    "maturity": {"status": "stable", "governedBy": "contractGovernance", "preferred": True},
    "compatibilityPolicy": {"status": "stable", "governedBy": "contractGovernance", "preferred": True},
    "contractGovernance": {"status": "stable", "governedBy": "contractGovernance", "preferred": True},
    "surfaceLifecycleMatrix": {"status": "stable", "governedBy": "contractGovernance", "preferred": True},
    "contractStatusMatrix": {"status": "stable", "governedBy": "contractGovernance", "preferred": True},
    "stabilityPolicy": {"status": "stable", "governedBy": "contractGovernance", "preferred": True}
  },
  "stabilityPolicy": {
    "additiveOnly": True,
    "ignoreUnknownFields": True,
    "preferredMachineReadableSurfaces": ["summaryJson", "statusJson", "doctorJson", "helpJson", "schema"],
    "preferredGlanceSurfaces": ["summaryCompact", "statusCompact", "doctor", "doctorCompact"],
    "preferredFieldSurfaces": ["summaryField", "statusField", "doctorField"],
    "notes": [
      "JSON contracts are intended to evolve additively; consumers should ignore unknown fields.",
      "Compact and field surfaces are stable for shell/operator consumption but do not replace the JSON contracts."
    ]
  },
  "recommendedConsumption": {
    "recommendedRunCommand": recommended_run_command,
    "firstCommands": first_commands,
    "summaryDisplayFields": [
      "overall.summaryDisplay",
      "overall.status",
      "overall.commandCount",
      "overall.surfaceCount",
    ],
    "metaDiscoverabilityPath": "metaDiscoverability",
    "metaDiscoverabilityContractPath": "contracts.metaDiscoverability",
    "catalogEntryPath": "catalogEntry",
    "catalogEntryContractPath": "contracts.catalogEntry",
    "surfaceCapabilitiesPath": "surfaceCapabilities",
    "surfaceCapabilitiesContractPath": "contracts.surfaceCapabilities",
    "stabilityPolicyPath": "stabilityPolicy",
    "stabilityPolicyContractPath": "contracts.stabilityPolicy",
    "surfaceProfilesPath": "surfaceProfiles",
    "surfaceProfilesContractPath": "contracts.surfaceProfiles",
    "consumerProfilesPath": "consumerProfiles",
    "consumerProfilesContractPath": "contracts.consumerProfiles",
    "profileSelectionGuidePath": "profileSelectionGuide",
    "profileSelectionGuideContractPath": "contracts.profileSelectionGuide",
    "profileSelectionTracePath": "profileSelectionTrace",
    "profileSelectionTraceContractPath": "contracts.profileSelectionTrace",
    "lifecyclePath": "lifecycle",
    "lifecycleContractPath": "contracts.lifecycle",
    "maturityPath": "maturity",
    "maturityContractPath": "contracts.maturity",
    "compatibilityPolicyPath": "compatibilityPolicy",
    "compatibilityPolicyContractPath": "contracts.compatibilityPolicy",
    "contractGovernancePath": "contractGovernance",
    "contractGovernanceContractPath": "contracts.contractGovernance",
    "surfaceLifecycleMatrixPath": "surfaceLifecycleMatrix",
    "surfaceLifecycleMatrixContractPath": "contracts.surfaceLifecycleMatrix",
    "contractStatusMatrixPath": "contractStatusMatrix",
    "contractStatusMatrixContractPath": "contracts.contractStatusMatrix",
  },
  "overall": {
    "status": "ok",
    "discoverabilityReady": True,
    "commandCount": len(commands),
    "surfaceCount": len(surfaces),
    "recommendedRunCommand": recommended_run_command,
    "summaryDisplay": f"commands:{len(commands)}|surfaces:{len(surfaces)}|default:{recommended_run_command}",
    "metaDisplay": "meta:summarySurfaceGuide+recommendedConsumption",
    "catalogReady": True,
    "surfacePolicyReady": True,
    "profileReady": True,
    "profileSelectionReady": True,
    "lifecycleReady": True,
    "maturityReady": True,
    "compatibilityReady": True,
    "contractGovernanceReady": True,
    "surfaceLifecycleMatrixReady": True,
    "contractStatusMatrixReady": True,
  },
}
print(json.dumps(payload, ensure_ascii=False, indent=2))
PY
}

print_examples() {
  cat <<'EOF'
./scripts/session-auth-runtime.sh --summary-compact
./scripts/session-auth-runtime.sh --status-compact
./scripts/session-auth-runtime.sh --doctor-compact
./scripts/session-auth-runtime.sh status --service both --compact
./scripts/session-auth-runtime.sh validate --service both --compact
./scripts/session-auth-runtime.sh reload --service both --consumer-token "$CONSUMER_ENTRY_INGRESS_TOKEN" --matrix-token "$MATRIX_ENTRY_INGRESS_TOKEN"
./scripts/session-auth-runtime.sh activate --candidate-file next.json --consumer-token "$CONSUMER_ENTRY_INGRESS_TOKEN" --matrix-token "$MATRIX_ENTRY_INGRESS_TOKEN"
./scripts/session-auth-runtime.sh rollback --backup-file ./run/session-auth-runtime-backups/<file>
./scripts/session-auth-runtime.sh history --summary --compact
./scripts/session-auth-runtime.sh last --require-converged
EOF
}

print_run_command() {
  printf '%s\n' './scripts/session-auth-runtime.sh --status-compact'
}

print_help_json() {
  python3 - <<PY
import json
print(json.dumps({
  "kind": "session-auth-runtime-help",
  "schemaVersion": 1,
  "name": "session-auth-runtime",
  "usage": "./scripts/session-auth-runtime.sh <command> [args...]",
  "summary": "Unified operator entrypoint for session-auth runtime authority operations.",
  "commands": {
    "status": {
      "summary": "Run coordinated live status via reload-session-auth-runtime.sh --action status",
      "delegatesTo": ${RELOAD_HELPER@Q},
      "forwardedArgs": True,
      "example": "./scripts/session-auth-runtime.sh status --service both --compact"
    },
    "validate": {
      "summary": "Run coordinated candidate validation via reload-session-auth-runtime.sh --action validate",
      "delegatesTo": ${RELOAD_HELPER@Q},
      "forwardedArgs": True,
      "example": "./scripts/session-auth-runtime.sh validate --service both --compact"
    },
    "reload": {
      "summary": "Run coordinated live reload via reload-session-auth-runtime.sh --action reload",
      "delegatesTo": ${RELOAD_HELPER@Q},
      "forwardedArgs": True,
      "example": "./scripts/session-auth-runtime.sh reload --service both --consumer-token \$CONSUMER_ENTRY_INGRESS_TOKEN --matrix-token \$MATRIX_ENTRY_INGRESS_TOKEN"
    },
    "activate": {
      "summary": "Promote a candidate registry file via activate-session-auth-runtime.sh",
      "delegatesTo": ${ACTIVATE_HELPER@Q},
      "forwardedArgs": True,
      "example": "./scripts/session-auth-runtime.sh activate --candidate-file next.json --consumer-token \$CONSUMER_ENTRY_INGRESS_TOKEN --matrix-token \$MATRIX_ENTRY_INGRESS_TOKEN"
    },
    "rollback": {
      "summary": "Restore a live registry backup via rollback-session-auth-runtime.sh",
      "delegatesTo": ${ROLLBACK_HELPER@Q},
      "forwardedArgs": True,
      "example": "./scripts/session-auth-runtime.sh rollback --backup-file ./run/session-auth-runtime-backups/<file>"
    },
    "history": {
      "summary": "Read append-only activation/rollback history",
      "delegatesTo": ${HISTORY_HELPER@Q},
      "forwardedArgs": True,
      "example": "./scripts/session-auth-runtime.sh history --summary --compact"
    },
    "last": {
      "summary": "Read latest activation summary status",
      "delegatesTo": ${STATUS_HELPER@Q},
      "forwardedArgs": True,
      "example": "./scripts/session-auth-runtime.sh last --require-converged"
    }
  },
  "surfaces": {
    "help": {"kind": "text/plain", "command": "./scripts/session-auth-runtime.sh --help"},
    "examples": {"kind": "text/plain", "command": "./scripts/session-auth-runtime.sh --examples"},
    "printRunCommand": {"kind": "text/plain", "command": "./scripts/session-auth-runtime.sh --print-run-command"},
    "helpJson": {"kind": "session-auth-runtime-help", "schemaVersion": 1, "command": "./scripts/session-auth-runtime.sh --help-json"},
    "summaryJson": {"kind": "session-auth-runtime-summary", "schemaVersion": 1, "command": "./scripts/session-auth-runtime.sh --summary-json"},
    "summaryCompact": {"kind": "text/plain", "command": "./scripts/session-auth-runtime.sh --summary-compact"},
    "summaryField": {"kind": "text/plain", "command": "./scripts/session-auth-runtime.sh --summary-field overall.status"},
    "schema": {"kind": "session-auth-runtime-schema", "schemaVersion": 1, "command": "./scripts/session-auth-runtime.sh --schema"},
    "statusJson": {"kind": "session-auth-runtime-status", "schemaVersion": 1, "command": "./scripts/session-auth-runtime.sh --status-json"},
    "statusCompact": {"kind": "text/plain", "command": "./scripts/session-auth-runtime.sh --status-compact"},
    "statusField": {"kind": "text/plain", "command": "./scripts/session-auth-runtime.sh --status-field overall.status"},
    "doctor": {"kind": "text/plain", "command": "./scripts/session-auth-runtime.sh --doctor"},
    "doctorJson": {"kind": "session-auth-runtime-doctor", "schemaVersion": 1, "command": "./scripts/session-auth-runtime.sh --doctor-json"},
    "doctorCompact": {"kind": "text/plain", "command": "./scripts/session-auth-runtime.sh --doctor-compact"},
    "doctorField": {"kind": "text/plain", "command": "./scripts/session-auth-runtime.sh --doctor-field overall.status"}
  },
  "recommendedConsumption": {
    "recommendedRunCommand": "./scripts/session-auth-runtime.sh --status-compact",
    "firstCommands": [
      "./scripts/session-auth-runtime.sh --status-compact",
      "./scripts/session-auth-runtime.sh --doctor-compact",
      "./scripts/session-auth-runtime.sh history --summary --compact"
    ],
    "fieldExamples": {
      "summary": "./scripts/session-auth-runtime.sh --summary-field overall.surfaceCount",
      "status": "./scripts/session-auth-runtime.sh --status-field overall.lastKnownStatus",
      "doctor": "./scripts/session-auth-runtime.sh --doctor-field overall.status"
    },
    "summaryDisplayFields": [
      "overall.summaryDisplay",
      "overall.status",
      "overall.requiresAttention"
    ],
    "metaDiscoverabilityPath": "metaDiscoverability",
    "metaDiscoverabilityContractPath": "contracts.metaDiscoverability",
    "catalogEntryPath": "catalogEntry",
    "catalogEntryContractPath": "contracts.catalogEntry",
    "surfaceCapabilitiesPath": "surfaceCapabilities",
    "surfaceCapabilitiesContractPath": "contracts.surfaceCapabilities",
    "stabilityPolicyPath": "stabilityPolicy",
    "stabilityPolicyContractPath": "contracts.stabilityPolicy",
    "surfaceProfilesPath": "surfaceProfiles",
    "surfaceProfilesContractPath": "contracts.surfaceProfiles",
    "consumerProfilesPath": "consumerProfiles",
    "consumerProfilesContractPath": "contracts.consumerProfiles",
    "profileSelectionGuidePath": "profileSelectionGuide",
    "profileSelectionGuideContractPath": "contracts.profileSelectionGuide",
    "profileSelectionTracePath": "profileSelectionTrace",
    "profileSelectionTraceContractPath": "contracts.profileSelectionTrace",
    "lifecyclePath": "lifecycle",
    "lifecycleContractPath": "contracts.lifecycle",
    "maturityPath": "maturity",
    "maturityContractPath": "contracts.maturity",
    "compatibilityPolicyPath": "compatibilityPolicy",
    "compatibilityPolicyContractPath": "contracts.compatibilityPolicy",
    "contractGovernancePath": "contractGovernance",
    "contractGovernanceContractPath": "contracts.contractGovernance",
    "surfaceLifecycleMatrixPath": "surfaceLifecycleMatrix",
    "surfaceLifecycleMatrixContractPath": "contracts.surfaceLifecycleMatrix",
    "contractStatusMatrixPath": "contractStatusMatrix",
    "contractStatusMatrixContractPath": "contracts.contractStatusMatrix"
  },
  "catalogEntry": {
    "kind": "session-auth-runtime-catalog-entry",
    "schemaVersion": 1,
    "entryId": "session-auth-runtime",
    "primaryCommand": "./scripts/session-auth-runtime.sh --summary-json",
    "preferredReadOrder": [
      "metaDiscoverability",
      "catalogEntry",
      "lifecycle",
      "maturity",
      "compatibilityPolicy",
      "contractGovernance",
      "surfaceLifecycleMatrix",
      "contractStatusMatrix",
      "surfaceCapabilities",
      "stabilityPolicy",
      "summarySurfaceGuide",
      "recommendedConsumption",
      "statusJson",
      "doctorJson",
      "helpJson",
      "schema"
    ],
    "surfaceKinds": {
      "helpJson": "session-auth-runtime-help",
      "summaryJson": "session-auth-runtime-summary",
      "statusJson": "session-auth-runtime-status",
      "doctorJson": "session-auth-runtime-doctor",
      "schema": "session-auth-runtime-schema"
    }
  },
  "lifecycle": {
    "currentPhase": "self-describing-operator-cli",
    "preferredEvolutionMode": "additive-only",
    "guarantees": [
      "Stable JSON contracts evolve additively.",
      "Compact and field surfaces remain shell/operator friendly.",
      "Help/schema/summary stay aligned as the main self-description trio."
    ]
  },
  "maturity": {
    "machineReadable": {
      "level": "stable",
      "surfaces": ["summaryJson", "helpJson", "schema", "statusJson", "doctorJson"]
    },
    "shellGlance": {
      "level": "stable",
      "surfaces": ["summaryCompact", "statusCompact", "doctorCompact"]
    },
    "fieldConsumption": {
      "level": "stable",
      "surfaces": ["summaryField", "statusField", "doctorField"]
    },
    "diagnosticText": {
      "level": "best_effort",
      "surfaces": ["doctor"]
    }
  },
  "compatibilityPolicy": {
    "contractStrategy": "additive-with-announced-deprecation",
    "unknownFieldHandling": "ignore",
    "stableAnchors": [
      "kind",
      "schemaVersion",
      "commands",
      "surfaces",
      "recommendedConsumption",
      "related"
    ],
    "pathCompatibility": {
      "preferredBehavior": "add-new-paths-before-shifting-read-order",
      "notes": [
        "Prefer additive path introduction over in-place replacement.",
        "Mirror new hints in summary, help, and schema before making them preferred."
      ]
    },
    "deprecation": {
      "mode": "announce-then-remove",
      "minimumBehavior": [
        "add a replacement path or surface first",
        "mirror the replacement in help_json.related and schema contracts",
        "keep deprecated paths during a transition window"
      ],
      "currentlyDeprecated": []
    }
  },
  "contractGovernance": {
    "governedBlocks": [
      "lifecycle",
      "maturity",
      "compatibilityPolicy",
      "stabilityPolicy"
    ],
    "preferredInspectionOrder": [
      "lifecycle",
      "maturity",
      "compatibilityPolicy",
      "stabilityPolicy"
    ],
    "operatorRule": "read-summary-then-contract-governance-before-consuming-lower-level-paths",
    "changeManagement": {
      "requiresMirroringAcross": ["summaryJson", "helpJson", "schema"],
      "requiresSmokeCoverage": True,
      "defaultPolicy": "additive-first"
    }
  },
  "surfaceLifecycleMatrix": {
    "helpJson": {"family": "machineReadable", "status": "stable", "preferred": False, "deprecated": False},
    "summaryJson": {"family": "machineReadable", "status": "stable", "preferred": True, "deprecated": False},
    "summaryCompact": {"family": "shellGlance", "status": "stable", "preferred": True, "deprecated": False},
    "summaryField": {"family": "fieldConsumption", "status": "stable", "preferred": True, "deprecated": False},
    "schema": {"family": "machineReadable", "status": "stable", "preferred": True, "deprecated": False},
    "statusJson": {"family": "machineReadable", "status": "stable", "preferred": True, "deprecated": False},
    "statusCompact": {"family": "shellGlance", "status": "stable", "preferred": True, "deprecated": False},
    "statusField": {"family": "fieldConsumption", "status": "stable", "preferred": True, "deprecated": False},
    "doctor": {"family": "diagnosticText", "status": "best_effort", "preferred": False, "deprecated": False},
    "doctorJson": {"family": "machineReadable", "status": "stable", "preferred": True, "deprecated": False},
    "doctorCompact": {"family": "shellGlance", "status": "stable", "preferred": True, "deprecated": False},
    "doctorField": {"family": "fieldConsumption", "status": "stable", "preferred": True, "deprecated": False}
  },
  "contractStatusMatrix": {
    "summarySurfaceGuide": {"status": "stable", "governedBy": "contractGovernance", "preferred": True},
    "metaDiscoverability": {"status": "stable", "governedBy": "contractGovernance", "preferred": True},
    "recommendedConsumption": {"status": "stable", "governedBy": "contractGovernance", "preferred": True},
    "catalogEntry": {"status": "stable", "governedBy": "contractGovernance", "preferred": True},
    "lifecycle": {"status": "stable", "governedBy": "contractGovernance", "preferred": True},
    "maturity": {"status": "stable", "governedBy": "contractGovernance", "preferred": True},
    "compatibilityPolicy": {"status": "stable", "governedBy": "contractGovernance", "preferred": True},
    "contractGovernance": {"status": "stable", "governedBy": "contractGovernance", "preferred": True},
    "surfaceLifecycleMatrix": {"status": "stable", "governedBy": "contractGovernance", "preferred": True},
    "contractStatusMatrix": {"status": "stable", "governedBy": "contractGovernance", "preferred": True},
    "stabilityPolicy": {"status": "stable", "governedBy": "contractGovernance", "preferred": True}
  },
  "related": {
    "catalogEntryPath": "catalogEntry",
    "catalogEntryContractPath": "contracts.catalogEntry",
    "metaDiscoverabilityPath": "metaDiscoverability",
    "metaDiscoverabilityContractPath": "contracts.metaDiscoverability",
    "surfaceCapabilitiesPath": "surfaceCapabilities",
    "surfaceCapabilitiesContractPath": "contracts.surfaceCapabilities",
    "stabilityPolicyPath": "stabilityPolicy",
    "stabilityPolicyContractPath": "contracts.stabilityPolicy",
    "surfaceProfilesPath": "surfaceProfiles",
    "surfaceProfilesContractPath": "contracts.surfaceProfiles",
    "consumerProfilesPath": "consumerProfiles",
    "consumerProfilesContractPath": "contracts.consumerProfiles",
    "profileSelectionGuidePath": "profileSelectionGuide",
    "profileSelectionGuideContractPath": "contracts.profileSelectionGuide",
    "profileSelectionTracePath": "profileSelectionTrace",
    "profileSelectionTraceContractPath": "contracts.profileSelectionTrace",
    "lifecyclePath": "lifecycle",
    "lifecycleContractPath": "contracts.lifecycle",
    "maturityPath": "maturity",
    "maturityContractPath": "contracts.maturity",
    "compatibilityPolicyPath": "compatibilityPolicy",
    "compatibilityPolicyContractPath": "contracts.compatibilityPolicy",
    "contractGovernancePath": "contractGovernance",
    "contractGovernanceContractPath": "contracts.contractGovernance",
    "surfaceLifecycleMatrixPath": "surfaceLifecycleMatrix",
    "surfaceLifecycleMatrixContractPath": "contracts.surfaceLifecycleMatrix",
    "contractStatusMatrixPath": "contractStatusMatrix",
    "contractStatusMatrixContractPath": "contracts.contractStatusMatrix",
    "summarySurfaceGuidePath": "summarySurfaceGuide",
    "summarySurfaceGuideContractPath": "contracts.summarySurfaceGuide",
    "recommendedConsumptionPath": "recommendedConsumption",
    "recommendedConsumptionContractPath": "contracts.recommendedConsumption",
    "reloadHelper": ${RELOAD_HELPER@Q},
    "activateHelper": ${ACTIVATE_HELPER@Q},
    "rollbackHelper": ${ROLLBACK_HELPER@Q},
    "historyHelper": ${HISTORY_HELPER@Q},
    "lastHelper": ${STATUS_HELPER@Q}
  }
}, ensure_ascii=False, indent=2))
PY
}

print_schema_json() {
  python3 - <<PY
import json
print(json.dumps({
  "kind": "session-auth-runtime-schema",
  "schemaVersion": 1,
  "name": "session-auth-runtime",
  "usage": "./scripts/session-auth-runtime.sh <command> [args...]",
  "contracts": {
    "helpJson": {
      "kind": "session-auth-runtime-help",
      "schemaVersion": 1,
      "requiredTopLevelFields": ["kind", "schemaVersion", "name", "usage", "summary", "commands", "surfaces", "related"]
    },
    "summaryJson": {
      "kind": "session-auth-runtime-summary",
      "schemaVersion": 1,
      "requiredTopLevelFields": ["kind", "schemaVersion", "name", "usage", "commands", "surfaces", "summarySurfaceGuide", "metaDiscoverability", "catalogEntry", "surfaceCapabilities", "surfaceProfiles", "consumerProfiles", "profileSelectionGuide", "profileSelectionTrace", "lifecycle", "maturity", "compatibilityPolicy", "contractGovernance", "surfaceLifecycleMatrix", "contractStatusMatrix", "stabilityPolicy", "recommendedConsumption", "overall"]
    },
    "catalogEntry": {
      "kind": "session-auth-runtime-catalog-entry",
      "schemaVersion": 1,
      "requiredTopLevelFields": ["kind", "schemaVersion", "entryId", "primaryCommand", "preferredReadOrder", "surfaceKinds"]
    },
    "summarySurfaceGuide": {
      "kind": "session-auth-runtime-summary-surface-guide",
      "schemaVersion": 1,
      "requiredTopLevelFields": ["summary", "status", "doctor", "discoverability"]
    },
    "schema": {
      "kind": "session-auth-runtime-schema",
      "schemaVersion": 1,
      "requiredTopLevelFields": ["kind", "schemaVersion", "name", "usage", "contracts", "commands", "delegation", "notes"]
    },
    "statusJson": {
      "kind": "session-auth-runtime-status",
      "schemaVersion": 1,
      "requiredTopLevelFields": ["kind", "schemaVersion", "defaults", "liveStatus", "lastSummary", "historySummary", "overall"]
    },
    "doctorJson": {
      "kind": "session-auth-runtime-doctor",
      "schemaVersion": 1,
      "requiredTopLevelFields": ["kind", "schemaVersion", "helpers", "defaults", "status", "checks", "overall"]
    },
    "recommendedConsumption": {
      "kind": "session-auth-runtime-recommended-consumption",
      "schemaVersion": 1,
      "requiredTopLevelFields": ["recommendedRunCommand", "firstCommands", "fieldExamples", "summaryDisplayFields", "metaDiscoverabilityPath", "metaDiscoverabilityContractPath", "catalogEntryPath", "catalogEntryContractPath", "surfaceCapabilitiesPath", "surfaceCapabilitiesContractPath", "stabilityPolicyPath", "stabilityPolicyContractPath", "surfaceProfilesPath", "surfaceProfilesContractPath", "consumerProfilesPath", "consumerProfilesContractPath", "profileSelectionGuidePath", "profileSelectionGuideContractPath", "profileSelectionTracePath", "profileSelectionTraceContractPath", "lifecyclePath", "lifecycleContractPath", "maturityPath", "maturityContractPath", "compatibilityPolicyPath", "compatibilityPolicyContractPath", "contractGovernancePath", "contractGovernanceContractPath", "surfaceLifecycleMatrixPath", "surfaceLifecycleMatrixContractPath", "contractStatusMatrixPath", "contractStatusMatrixContractPath"]
    },
    "metaDiscoverability": {
      "kind": "session-auth-runtime-meta-discoverability",
      "schemaVersion": 1,
      "requiredTopLevelFields": ["summarySurfaceGuidePath", "summarySurfaceGuideContractPath", "recommendedConsumptionPath", "recommendedConsumptionContractPath", "catalogEntryPath", "catalogEntryContractPath", "surfaceCapabilitiesPath", "surfaceCapabilitiesContractPath", "stabilityPolicyPath", "stabilityPolicyContractPath", "surfaceProfilesPath", "surfaceProfilesContractPath", "consumerProfilesPath", "consumerProfilesContractPath", "profileSelectionGuidePath", "profileSelectionGuideContractPath", "profileSelectionTracePath", "profileSelectionTraceContractPath", "lifecyclePath", "lifecycleContractPath", "maturityPath", "maturityContractPath", "compatibilityPolicyPath", "compatibilityPolicyContractPath", "contractGovernancePath", "contractGovernanceContractPath", "surfaceLifecycleMatrixPath", "surfaceLifecycleMatrixContractPath", "contractStatusMatrixPath", "contractStatusMatrixContractPath", "helpJsonCommand", "schemaCommand", "summaryJsonCommand"]
    },
    "surfaceCapabilities": {
      "kind": "session-auth-runtime-surface-capabilities",
      "schemaVersion": 1,
      "requiredTopLevelFields": ["helpJson", "summaryJson", "summaryCompact", "summaryField", "statusJson", "statusCompact", "statusField", "doctor", "doctorJson", "doctorCompact", "doctorField", "schema"]
    },
    "surfaceProfiles": {
      "kind": "session-auth-runtime-surface-profiles",
      "schemaVersion": 1,
      "requiredTopLevelFields": ["machine_reader", "shell_glance", "field_reader", "operator_debug"]
    },
    "consumerProfiles": {
      "kind": "session-auth-runtime-consumer-profiles",
      "schemaVersion": 1,
      "requiredTopLevelFields": ["machine_parser", "shell_operator", "schema_reader", "operator_debugger"]
    },
    "profileSelectionGuide": {
      "kind": "session-auth-runtime-profile-selection-guide",
      "schemaVersion": 1,
      "requiredTopLevelFields": ["ifYouNeed", "selectionOrder"]
    },
    "profileSelectionTrace": {
      "kind": "session-auth-runtime-profile-selection-trace",
      "schemaVersion": 1,
      "requiredTopLevelFields": ["machine_parser", "shell_operator", "schema_reader", "operator_debugger"]
    },
    "lifecycle": {
      "kind": "session-auth-runtime-lifecycle",
      "schemaVersion": 1,
      "requiredTopLevelFields": ["currentPhase", "preferredEvolutionMode", "guarantees"]
    },
    "maturity": {
      "kind": "session-auth-runtime-maturity",
      "schemaVersion": 1,
      "requiredTopLevelFields": ["machineReadable", "shellGlance", "fieldConsumption", "diagnosticText"]
    },
    "compatibilityPolicy": {
      "kind": "session-auth-runtime-compatibility-policy",
      "schemaVersion": 1,
      "requiredTopLevelFields": ["contractStrategy", "unknownFieldHandling", "stableAnchors", "pathCompatibility", "deprecation"]
    },
    "contractGovernance": {
      "kind": "session-auth-runtime-contract-governance",
      "schemaVersion": 1,
      "requiredTopLevelFields": ["governedBlocks", "preferredInspectionOrder", "operatorRule", "changeManagement"]
    },
    "surfaceLifecycleMatrix": {
      "kind": "session-auth-runtime-surface-lifecycle-matrix",
      "schemaVersion": 1,
      "requiredTopLevelFields": ["helpJson", "summaryJson", "summaryCompact", "summaryField", "schema", "statusJson", "statusCompact", "statusField", "doctor", "doctorJson", "doctorCompact", "doctorField"]
    },
    "contractStatusMatrix": {
      "kind": "session-auth-runtime-contract-status-matrix",
      "schemaVersion": 1,
      "requiredTopLevelFields": ["summarySurfaceGuide", "metaDiscoverability", "recommendedConsumption", "catalogEntry", "lifecycle", "maturity", "compatibilityPolicy", "contractGovernance", "surfaceLifecycleMatrix", "contractStatusMatrix", "stabilityPolicy"]
    },
    "stabilityPolicy": {
      "kind": "session-auth-runtime-stability-policy",
      "schemaVersion": 1,
      "requiredTopLevelFields": ["additiveOnly", "ignoreUnknownFields", "preferredMachineReadableSurfaces", "preferredGlanceSurfaces", "preferredFieldSurfaces", "notes"]
    },
    "summaryCompact": {
      "kind": "text/plain",
      "schemaVersion": 1,
      "fields": ["overall.status", "overall.discoverabilityReady", "overall.commandCount", "overall.surfaceCount", "overall.recommendedRunCommand", "overall.summaryDisplay"]
    },
    "statusCompact": {
      "kind": "text/plain",
      "schemaVersion": 1,
      "fields": ["overall.status", "overall.requiresAttention", "overall.liveStatusOk", "overall.lastSummaryOk", "overall.historySummaryOk", "overall.lastKnownStatus", "overall.historyLatestStatus", "overall.summaryDisplay"]
    },
    "doctorCompact": {
      "kind": "text/plain",
      "schemaVersion": 1,
      "fields": ["overall.status", "overall.requiresAttention", "checks.helpersExecutable", "checks.liveStatusReachable", "checks.lastSummaryReadable", "checks.historySummaryReadable", "overall.summaryDisplay"]
    }
  },
  "commands": {
    "status": {"delegatesTo": ${RELOAD_HELPER@Q}, "fixedArgs": ["--action", "status"]},
    "validate": {"delegatesTo": ${RELOAD_HELPER@Q}, "fixedArgs": ["--action", "validate"]},
    "reload": {"delegatesTo": ${RELOAD_HELPER@Q}, "fixedArgs": ["--action", "reload"]},
    "activate": {"delegatesTo": ${ACTIVATE_HELPER@Q}, "fixedArgs": []},
    "rollback": {"delegatesTo": ${ROLLBACK_HELPER@Q}, "fixedArgs": []},
    "history": {"delegatesTo": ${HISTORY_HELPER@Q}, "fixedArgs": []},
    "last": {"delegatesTo": ${STATUS_HELPER@Q}, "fixedArgs": []}
  },
  "delegation": {
    "model": "thin-router",
    "preservesExistingHelperSemantics": True,
    "forwardsRemainingArgs": True
  },
  "notes": [
    "Top-level --examples, --print-run-command, --help-json, --summary-json, --schema, --status-json, --doctor, and --doctor-json are handled directly by this wrapper.",
    "All subcommands forward remaining args unchanged to their delegated helper."
  ],
  "recommendedConsumption": {
    "recommendedRunCommand": "./scripts/session-auth-runtime.sh --status-compact",
    "recommendedConsumptionPath": "recommendedConsumption",
    "contractPath": "contracts.recommendedConsumption",
    "summarySurfaceGuidePath": "summarySurfaceGuide",
    "summarySurfaceGuideContractPath": "contracts.summarySurfaceGuide",
    "metaDiscoverabilityPath": "metaDiscoverability",
    "metaDiscoverabilityContractPath": "contracts.metaDiscoverability",
    "catalogEntryPath": "catalogEntry",
    "catalogEntryContractPath": "contracts.catalogEntry",
    "surfaceCapabilitiesPath": "surfaceCapabilities",
    "surfaceCapabilitiesContractPath": "contracts.surfaceCapabilities",
    "stabilityPolicyPath": "stabilityPolicy",
    "stabilityPolicyContractPath": "contracts.stabilityPolicy",
    "surfaceProfilesPath": "surfaceProfiles",
    "surfaceProfilesContractPath": "contracts.surfaceProfiles",
    "consumerProfilesPath": "consumerProfiles",
    "consumerProfilesContractPath": "contracts.consumerProfiles",
    "profileSelectionGuidePath": "profileSelectionGuide",
    "profileSelectionGuideContractPath": "contracts.profileSelectionGuide",
    "profileSelectionTracePath": "profileSelectionTrace",
    "profileSelectionTraceContractPath": "contracts.profileSelectionTrace",
    "lifecyclePath": "lifecycle",
    "lifecycleContractPath": "contracts.lifecycle",
    "maturityPath": "maturity",
    "maturityContractPath": "contracts.maturity",
    "compatibilityPolicyPath": "compatibilityPolicy",
    "compatibilityPolicyContractPath": "contracts.compatibilityPolicy",
    "contractGovernancePath": "contractGovernance",
    "contractGovernanceContractPath": "contracts.contractGovernance",
    "surfaceLifecycleMatrixPath": "surfaceLifecycleMatrix",
    "surfaceLifecycleMatrixContractPath": "contracts.surfaceLifecycleMatrix",
    "contractStatusMatrixPath": "contractStatusMatrix",
    "contractStatusMatrixContractPath": "contracts.contractStatusMatrix",
    "firstCommands": [
      "./scripts/session-auth-runtime.sh --status-compact",
      "./scripts/session-auth-runtime.sh --doctor-compact",
      "./scripts/session-auth-runtime.sh history --summary --compact"
    ]
  }
}, ensure_ascii=False, indent=2))
PY
}

render_named_surface() {
  local payload_json="$1"
  local surface_name="$2"
  local output_mode="$3"
  local field_path="${4:-}"
  python3 - "$payload_json" "$surface_name" "$output_mode" "$field_path" <<'PY'
from __future__ import annotations
import json, shlex, sys
payload = json.loads(sys.argv[1])
surface_name = sys.argv[2]
output_mode = sys.argv[3]
field_path = sys.argv[4]

def lookup(value, dotted: str):
    current = value
    for part in dotted.split('.'):
        if isinstance(current, dict) and part in current:
            current = current[part]
        else:
            raise KeyError(dotted)
    return current

def format_scalar(value):
    if value is None:
        return 'null'
    if isinstance(value, bool):
        return str(value).lower()
    return str(value)

overall = payload.get('overall') or {}
if output_mode == 'json':
    print(json.dumps(payload, ensure_ascii=False, indent=2))
elif output_mode == 'compact':
    if surface_name == 'status':
        parts = [
            f"status={format_scalar(overall.get('status'))}",
            f"attention={format_scalar(overall.get('requiresAttention'))}",
            f"live={format_scalar(overall.get('liveStatusOk'))}",
            f"last={format_scalar(overall.get('lastSummaryOk'))}",
            f"history={format_scalar(overall.get('historySummaryOk'))}",
            f"lastStatus={format_scalar(overall.get('lastKnownStatus'))}",
            f"historyStatus={format_scalar(overall.get('historyLatestStatus'))}",
            f"summary={shlex.quote(format_scalar(overall.get('summaryDisplay')))}",
        ]
    elif surface_name == 'doctor':
        checks = payload.get('checks') or {}
        parts = [
            f"status={format_scalar(overall.get('status'))}",
            f"attention={format_scalar(overall.get('requiresAttention'))}",
            f"helpers={format_scalar(checks.get('helpersExecutable'))}",
            f"live={format_scalar(checks.get('liveStatusReachable'))}",
            f"last={format_scalar(checks.get('lastSummaryReadable'))}",
            f"history={format_scalar(checks.get('historySummaryReadable'))}",
            f"summary={shlex.quote(format_scalar(overall.get('summaryDisplay')))}",
        ]
    else:
        parts = [
            f"status={format_scalar(overall.get('status'))}",
            f"discoverability={format_scalar(overall.get('discoverabilityReady'))}",
            f"commands={format_scalar(overall.get('commandCount'))}",
            f"surfaces={format_scalar(overall.get('surfaceCount'))}",
            f"run={shlex.quote(format_scalar(overall.get('recommendedRunCommand')))}",
            f"summary={shlex.quote(format_scalar(overall.get('summaryDisplay')))}",
        ]
    print(' '.join(parts))
elif output_mode == 'field':
    try:
        value = lookup(payload, field_path)
    except KeyError:
        raise SystemExit(f"Error: unsupported field path: {field_path}")
    if isinstance(value, (dict, list)):
        print(json.dumps(value, ensure_ascii=False))
    elif isinstance(value, bool):
        print(str(value).lower())
    elif value is None:
        print('null')
    else:
        print(value)
else:
    raise SystemExit(f"Error: unsupported output mode: {output_mode}")
PY
}

print_summary_json() {
  collect_summary_json
}

print_summary_compact() {
  local summary_json
  summary_json="$(collect_summary_json)"
  render_named_surface "$summary_json" summary compact
}

print_summary_field() {
  local field_path="$1"
  local summary_json
  summary_json="$(collect_summary_json)"
  render_named_surface "$summary_json" summary field "$field_path"
}

print_status_json() {
  collect_status_json
}

print_status_compact() {
  local status_json
  status_json="$(collect_status_json)"
  render_named_surface "$status_json" status compact
}

print_status_field() {
  local field_path="$1"
  local status_json
  status_json="$(collect_status_json)"
  render_named_surface "$status_json" status field "$field_path"
}

print_doctor_json() {
  collect_doctor_json
}

print_doctor_compact() {
  local doctor_json
  doctor_json="$(collect_doctor_json)"
  render_named_surface "$doctor_json" doctor compact
}

print_doctor_field() {
  local field_path="$1"
  local doctor_json
  doctor_json="$(collect_doctor_json)"
  render_named_surface "$doctor_json" doctor field "$field_path"
}

print_doctor_text() {
  local doctor_json
  doctor_json="$(collect_doctor_json)"
  python3 - "$doctor_json" <<'PY'
from __future__ import annotations
import json, sys
payload = json.loads(sys.argv[1])
status = payload.get('status') or {}
overall = payload.get('overall') or {}
checks = payload.get('checks') or {}
status_overall = status.get('overall') or {}
print(f"kind: {payload.get('kind')}")
print(f"schemaVersion: {payload.get('schemaVersion')}")
print(f"status: {overall.get('status')}")
print(f"requiresAttention: {overall.get('requiresAttention')}")
print(f"summary: {overall.get('summaryDisplay')}")
print(f"helpersExecutable: {checks.get('helpersExecutable')}")
print(f"liveStatusReachable: {checks.get('liveStatusReachable')}")
print(f"lastSummaryReadable: {checks.get('lastSummaryReadable')}")
print(f"historySummaryReadable: {checks.get('historySummaryReadable')}")
print(f"liveSummary: {status_overall.get('summaryDisplay')}")
PY
}

usage() {
  cat <<'EOF'
Usage: ./scripts/session-auth-runtime.sh <command> [args...]

Unified operator entrypoint for session-auth runtime authority operations.

Commands:
  status [args...]      Run coordinated live status via reload-session-auth-runtime.sh --action status
  validate [args...]    Run coordinated candidate validation via reload-session-auth-runtime.sh --action validate
  reload [args...]      Run coordinated live reload via reload-session-auth-runtime.sh --action reload
  activate [args...]    Promote a candidate registry file via activate-session-auth-runtime.sh
  rollback [args...]    Restore a live registry backup via rollback-session-auth-runtime.sh
  history [args...]     Read append-only activation/rollback history
  last [args...]        Read latest activation summary status
  help                  Show this help

Top-level machine-readable surfaces:
  --examples            Print example commands
  --print-run-command   Print the recommended default operator command
  --help-json           Print machine-readable help
  --summary-json        Print top-level self-summary JSON
  --summary-compact     Print a single-line self-summary
  --summary-field <p>   Print a single field from self-summary JSON
  --schema              Print machine-readable schema
  --status-json         Print aggregated operator status JSON
  --status-compact      Print a single-line status summary
  --status-field <p>    Print a single field from aggregated status JSON
  --doctor              Print a text doctor snapshot
  --doctor-json         Print a machine-readable doctor snapshot
  --doctor-compact      Print a single-line doctor summary
  --doctor-field <p>    Print a single field from doctor JSON

Examples:
  ./scripts/session-auth-runtime.sh status --service both --compact
  ./scripts/session-auth-runtime.sh reload --service both --consumer-token "$CONSUMER_ENTRY_INGRESS_TOKEN" --matrix-token "$MATRIX_ENTRY_INGRESS_TOKEN"
  ./scripts/session-auth-runtime.sh activate --candidate-file next.json --consumer-token "$CONSUMER_ENTRY_INGRESS_TOKEN" --matrix-token "$MATRIX_ENTRY_INGRESS_TOKEN"
  ./scripts/session-auth-runtime.sh rollback --backup-file ./run/session-auth-runtime-backups/<file>
  ./scripts/session-auth-runtime.sh history --summary --compact
  ./scripts/session-auth-runtime.sh last --require-converged
  ./scripts/session-auth-runtime.sh --examples
  ./scripts/session-auth-runtime.sh --print-run-command
  ./scripts/session-auth-runtime.sh --summary-json
  ./scripts/session-auth-runtime.sh --summary-compact
  ./scripts/session-auth-runtime.sh --summary-field overall.surfaceCount
  ./scripts/session-auth-runtime.sh --status-json
  ./scripts/session-auth-runtime.sh --status-compact
  ./scripts/session-auth-runtime.sh --status-field overall.status
  ./scripts/session-auth-runtime.sh --doctor-json
  ./scripts/session-auth-runtime.sh --doctor-compact
  ./scripts/session-auth-runtime.sh --doctor-field overall.status
EOF
}

case "${1:-}" in
  --examples)
    print_examples
    exit 0
    ;;
  --print-run-command)
    print_run_command
    exit 0
    ;;
  --help-json)
    print_help_json
    exit 0
    ;;
  --summary-json)
    print_summary_json
    exit 0
    ;;
  --summary-compact)
    print_summary_compact
    exit 0
    ;;
  --summary-field)
    [[ $# -ge 2 ]] || { echo "Error: --summary-field requires a path" >&2; exit 2; }
    print_summary_field "$2"
    exit 0
    ;;
  --schema)
    print_schema_json
    exit 0
    ;;
  --status-json)
    print_status_json
    exit 0
    ;;
  --status-compact)
    print_status_compact
    exit 0
    ;;
  --status-field)
    [[ $# -ge 2 ]] || { echo "Error: --status-field requires a path" >&2; exit 2; }
    print_status_field "$2"
    exit 0
    ;;
  --doctor)
    print_doctor_text
    exit 0
    ;;
  --doctor-json)
    print_doctor_json
    exit 0
    ;;
  --doctor-compact)
    print_doctor_compact
    exit 0
    ;;
  --doctor-field)
    [[ $# -ge 2 ]] || { echo "Error: --doctor-field requires a path" >&2; exit 2; }
    print_doctor_field "$2"
    exit 0
    ;;
  help|-h|--help|"")
    usage
    exit 0
    ;;
esac

command="$1"
shift

case "$command" in
  status)
    exec "$RELOAD_HELPER" --action status "$@"
    ;;
  validate)
    exec "$RELOAD_HELPER" --action validate "$@"
    ;;
  reload)
    exec "$RELOAD_HELPER" --action reload "$@"
    ;;
  activate)
    exec "$ACTIVATE_HELPER" "$@"
    ;;
  rollback)
    exec "$ROLLBACK_HELPER" "$@"
    ;;
  history)
    exec "$HISTORY_HELPER" "$@"
    ;;
  last)
    exec "$STATUS_HELPER" "$@"
    ;;
  *)
    echo "Error: unknown session-auth-runtime command: $command" >&2
    usage >&2
    exit 1
    ;;
esac
