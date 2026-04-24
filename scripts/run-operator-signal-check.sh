#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd -- "$SCRIPT_DIR/.." && pwd)"
STATE_DIR="${OPERATOR_SIGNAL_STATE_DIR:-$PROJECT_ROOT/run/operator-signals}"
CHECK_SCRIPT="${CHECK_OPERATOR_SIGNALS_SCRIPT:-$SCRIPT_DIR/check-operator-signals.sh}"
NOTIFY_ON="${OPERATOR_SIGNAL_NOTIFY_ON:-critical}"
NOTIFY_COMMAND="${OPERATOR_SIGNAL_NOTIFY_COMMAND:-}"
NOTIFY_WARN_COMMAND="${OPERATOR_SIGNAL_NOTIFY_WARN_COMMAND:-}"
NOTIFY_CRITICAL_COMMAND="${OPERATOR_SIGNAL_NOTIFY_CRITICAL_COMMAND:-}"
NOTIFY_RECOVERY="${OPERATOR_SIGNAL_NOTIFY_RECOVERY:-1}"
NOTIFY_RECOVERY_COMMAND="${OPERATOR_SIGNAL_NOTIFY_RECOVERY_COMMAND:-}"
NOTIFY_POLICY_JSON="${OPERATOR_SIGNAL_NOTIFY_POLICY_JSON:-}"
NOTIFY_POLICY_BUNDLE="${OPERATOR_SIGNAL_NOTIFY_POLICY_BUNDLE:-}"
NOTIFY_POLICY_PROFILE="${OPERATOR_SIGNAL_NOTIFY_POLICY_PROFILE:-}"
RENDER_POLICY_BUNDLE_SCRIPT="${RENDER_OPERATOR_SIGNAL_POLICY_BUNDLE_SCRIPT:-$SCRIPT_DIR/render-operator-signal-policy-bundle.sh}"
NOTIFY_SIGNAL_COMMANDS_JSON="${OPERATOR_SIGNAL_NOTIFY_SIGNAL_COMMANDS_JSON:-}"
NOTIFY_CHANGES_ONLY="${OPERATOR_SIGNAL_NOTIFY_CHANGES_ONLY:-1}"
NOTIFY_REMINDER_SECS="${OPERATOR_SIGNAL_NOTIFY_REMINDER_SECS:-1800}"
OUTPUT_MODE="pretty"

usage() {
  cat <<'EOF'
Usage: scripts/run-operator-signal-check.sh [--compact] [--pretty]

Runs check-operator-signals.sh, persists the latest result under run/operator-signals,
and optionally invokes a notification command.

Exit codes follow the underlying check script:
  0 = ok
  1 = warn
  2 = critical

Optional env:
  OPERATOR_SIGNAL_STATE_DIR
  CHECK_OPERATOR_SIGNALS_SCRIPT
  OPERATOR_SIGNAL_NOTIFY_ON=never|critical|warn|always
  OPERATOR_SIGNAL_NOTIFY_COMMAND="/path/to/command"
  OPERATOR_SIGNAL_NOTIFY_WARN_COMMAND="/path/to/warn-command"
  OPERATOR_SIGNAL_NOTIFY_CRITICAL_COMMAND="/path/to/critical-command"
  OPERATOR_SIGNAL_NOTIFY_RECOVERY=0|1
  OPERATOR_SIGNAL_NOTIFY_RECOVERY_COMMAND="/path/to/recovery-command"
  OPERATOR_SIGNAL_NOTIFY_POLICY_JSON='[{"name":"rule","matchAll":["service:name"],"command":"/path/to/command"}]'
  OPERATOR_SIGNAL_NOTIFY_POLICY_BUNDLE='baseline'  (or comma-separated bundles like entry-identity,monitoring-deploy)
  OPERATOR_SIGNAL_NOTIFY_POLICY_PROFILE='default'  (or comma-separated profiles like default,identity,deploy)
  RENDER_OPERATOR_SIGNAL_POLICY_BUNDLE_SCRIPT=./scripts/render-operator-signal-policy-bundle.sh
  OPERATOR_SIGNAL_NOTIFY_SIGNAL_COMMANDS_JSON='{"service:name":"/path/to/command"}'
  OPERATOR_SIGNAL_NOTIFY_CHANGES_ONLY=0|1
  OPERATOR_SIGNAL_NOTIFY_REMINDER_SECS=<seconds, 0 disables reminders>

When a notify command is selected, the wrapper passes the result JSON on stdin and also sets:
  OPERATOR_SIGNAL_OVERALL
  OPERATOR_SIGNAL_EXIT_CODE
  OPERATOR_SIGNAL_RESULT_PATH
  OPERATOR_SIGNAL_NOTIFY_REASON
  OPERATOR_SIGNAL_NOTIFY_SEVERITY
  OPERATOR_SIGNAL_NOTIFY_ROUTE
  OPERATOR_SIGNAL_NOTIFY_POLICY_NAME
  OPERATOR_SIGNAL_NOTIFY_POLICY_SUMMARY
  OPERATOR_SIGNAL_NOTIFY_POLICY_SUMMARY_ULTRA_SHORT
  OPERATOR_SIGNAL_NOTIFY_POLICY_SUMMARY_FULL
  OPERATOR_SIGNAL_NOTIFY_POLICY_ESCALATED
  OPERATOR_SIGNAL_NOTIFY_POLICY_GROUP_ESCALATED
  OPERATOR_SIGNAL_NOTIFY_SIGNAL_KEY
  OPERATOR_SIGNAL_NOTIFY_SIGNAL_KEYS
  OPERATOR_SIGNAL_NOTIFY_PREVIOUS_SEVERITY
  OPERATOR_SIGNAL_NOTIFY_PREVIOUS_SIGNAL_KEY
EOF
}

iso_to_epoch() {
  local iso="$1"
  local parsed
  if [[ -n "$iso" ]] && parsed="$(date -d "$iso" +%s 2>/dev/null)"; then
    printf '%s\n' "$parsed"
  else
    date +%s
  fi
}

epoch_to_iso() {
  local epoch="$1"
  date -Iseconds -d "@$epoch"
}

severity_from_exit() {
  local exit_code="$1"
  if [[ "$exit_code" -ge 2 ]]; then
    printf 'critical\n'
  elif [[ "$exit_code" -ge 1 ]]; then
    printf 'warn\n'
  else
    printf 'ok\n'
  fi
}

trim_whitespace() {
  local value="$1"
  value="${value#"${value%%[![:space:]]*}"}"
  value="${value%"${value##*[![:space:]]}"}"
  printf '%s\n' "$value"
}

if [[ -z "$NOTIFY_POLICY_JSON" && ( -n "$NOTIFY_POLICY_BUNDLE" || -n "$NOTIFY_POLICY_PROFILE" ) ]]; then
  if [[ ! -x "$RENDER_POLICY_BUNDLE_SCRIPT" ]]; then
    if [[ ! -f "$RENDER_POLICY_BUNDLE_SCRIPT" ]]; then
      echo "render policy bundle script not found: $RENDER_POLICY_BUNDLE_SCRIPT" >&2
      exit 64
    fi
  fi
  bundle_args=()
  if [[ -n "$NOTIFY_POLICY_PROFILE" ]]; then
    IFS=',' read -r -a raw_policy_profiles <<< "$NOTIFY_POLICY_PROFILE"
    for raw_profile in "${raw_policy_profiles[@]}"; do
      profile="$(trim_whitespace "$raw_profile")"
      [[ -n "$profile" ]] || continue
      bundle_args+=(--profile "$profile")
    done
  fi
  if [[ -n "$NOTIFY_POLICY_BUNDLE" ]]; then
    IFS=',' read -r -a raw_policy_bundles <<< "$NOTIFY_POLICY_BUNDLE"
    for raw_bundle in "${raw_policy_bundles[@]}"; do
      bundle="$(trim_whitespace "$raw_bundle")"
      [[ -n "$bundle" ]] || continue
      bundle_args+=(--bundle "$bundle")
    done
  fi
  if [[ "${#bundle_args[@]}" -eq 0 ]]; then
    echo "invalid OPERATOR_SIGNAL_NOTIFY_POLICY_PROFILE/OPERATOR_SIGNAL_NOTIFY_POLICY_BUNDLE: no names resolved" >&2
    exit 64
  fi
  NOTIFY_POLICY_JSON="$("$RENDER_POLICY_BUNDLE_SCRIPT" "${bundle_args[@]}")"
fi

threshold_met_for_mode() {
  local mode="$1"
  local exit_code="$2"
  case "$mode" in
    never)
      return 1
      ;;
    critical)
      [[ "$exit_code" -ge 2 ]]
      ;;
    warn)
      [[ "$exit_code" -ge 1 ]]
      ;;
    always)
      return 0
      ;;
    *)
      return 2
      ;;
  esac
}

select_notify_command() {
  local severity="$1"
  local previous_severity="${2:-}"
  local selected_command=""
  local selected_route="missing"

  case "$severity" in
    critical)
      if [[ -n "$NOTIFY_CRITICAL_COMMAND" ]]; then
        selected_command="$NOTIFY_CRITICAL_COMMAND"
        selected_route="critical"
      elif [[ -n "$NOTIFY_COMMAND" ]]; then
        selected_command="$NOTIFY_COMMAND"
        selected_route="default"
      fi
      ;;
    warn)
      if [[ -n "$NOTIFY_WARN_COMMAND" ]]; then
        selected_command="$NOTIFY_WARN_COMMAND"
        selected_route="warn"
      elif [[ -n "$NOTIFY_COMMAND" ]]; then
        selected_command="$NOTIFY_COMMAND"
        selected_route="default"
      fi
      ;;
    ok)
      if [[ -n "$NOTIFY_RECOVERY_COMMAND" ]]; then
        selected_command="$NOTIFY_RECOVERY_COMMAND"
        selected_route="recovery"
      elif [[ -n "$NOTIFY_COMMAND" ]]; then
        selected_command="$NOTIFY_COMMAND"
        selected_route="default"
      elif [[ "$previous_severity" == "critical" && -n "$NOTIFY_CRITICAL_COMMAND" ]]; then
        selected_command="$NOTIFY_CRITICAL_COMMAND"
        selected_route="critical_fallback"
      elif [[ "$previous_severity" == "warn" && -n "$NOTIFY_WARN_COMMAND" ]]; then
        selected_command="$NOTIFY_WARN_COMMAND"
        selected_route="warn_fallback"
      fi
      ;;
  esac

  printf '%s\n%s\n' "$selected_command" "$selected_route"
}

update_policy_observation_state() {
  local body_file="$1"

  if [[ -z "$NOTIFY_POLICY_JSON" ]]; then
    rm -f "$POLICY_OBSERVATION_STATE_PATH"
    return 0
  fi

  local previous_state='{}'
  if [[ -f "$POLICY_OBSERVATION_STATE_PATH" ]]; then
    previous_state="$(jq -c '.' "$POLICY_OBSERVATION_STATE_PATH" 2>/dev/null || echo '{}')"
  fi

  jq -n \
    --argjson rules "$NOTIFY_POLICY_JSON" \
    --argjson prev "$previous_state" \
    --argjson now "$NOW_EPOCH" \
    --slurpfile body "$body_file" '
      [($body[0].alerts // [])[]
        | {
            signal_key: ((.service // "") + ":" + (.name // "")),
            severity_rank: (if .severity == "critical" then 0 elif .severity == "warn" then 1 else 2 end)
          }
      ] as $alerts
      | ($alerts | map(.signal_key)) as $keys
      | ($prev.__groups // {}) as $prevGroups
      | [ $rules[]
          | . as $rule
          | ($rule.matchAll // []) as $matchAll
          | ($rule.matchAny // []) as $matchAny
          | ($rule.matchNone // []) as $matchNone
          | ($rule.name // ($matchAll[0] // $matchAny[0] // $matchNone[0] // "policy")) as $name
          | ($prev[$name] // {}) as $prevState
          | (($rule.groupKey // "")) as $groupKey
          | (($rule.groupOccurrenceWindowSeconds // ($rule.occurrenceWindowSeconds // 0)) | floor) as $groupOccurrenceWindowSeconds
          | (($rule.groupMaxGapSeconds // ($rule.maxGapSeconds // 0)) | floor) as $groupMaxGapSeconds
          | (($rule.maxGapSeconds // 0) | floor) as $maxGapSeconds
          | (($rule.occurrenceWindowSeconds // 0) | floor) as $occurrenceWindowSeconds
          | ($prevState.last_seen_at_epoch // 0) as $previousLastSeen
          | ((($maxGapSeconds > 0) and ($previousLastSeen > 0) and (($now - $previousLastSeen) > $maxGapSeconds))) as $gapReset
          | (if $gapReset then [] else ($prevState.occurrence_timestamps // []) end) as $baseTimestamps
          | (($baseTimestamps + [$now])
              | map(select(type == "number"))
              | map(floor)
              | sort
              | if $occurrenceWindowSeconds > 0 then map(select(. >= ($now - $occurrenceWindowSeconds))) else . end
            ) as $timestamps
          | select(($rule.command // "") != "")
          | select(([$matchAll[]? as $matchAllKey | select($keys | index($matchAllKey)) | $matchAllKey] | length) == ($matchAll | length))
          | select((($matchAny | length) == 0) or ([ $matchAny[]? as $matchAnyKey | select($keys | index($matchAnyKey)) | $matchAnyKey ] | length) > 0)
          | select(([ $matchNone[]? as $matchNoneKey | select($keys | index($matchNoneKey)) | $matchNoneKey ] | length) == 0)
          | {
              name: $name,
              group_key: $groupKey,
              group_occurrence_window_seconds: $groupOccurrenceWindowSeconds,
              group_max_gap_seconds: $groupMaxGapSeconds,
              primary_signal_key: ($matchAll[0] // $matchAny[0] // ($alerts | sort_by(.severity_rank, .signal_key) | .[0].signal_key // "")),
              signal_keys: ((($matchAll + $matchAny) | unique) | join(",")),
              first_seen_at_epoch: (if $gapReset then $now else ($prevState.first_seen_at_epoch // $now) end),
              last_seen_at_epoch: $now,
              occurrences: ($timestamps | length),
              occurrence_timestamps: $timestamps,
              max_gap_seconds: $maxGapSeconds,
              occurrence_window_seconds: $occurrenceWindowSeconds
            }
        ] as $matchedPolicies
      | ($matchedPolicies
          | reduce .[] as $item ({};
              if (($item.group_key // "") | length) == 0 then
                .
              else
                ($item.group_key) as $groupKey
                | ($prevGroups[$groupKey] // {}) as $prevGroup
                | ($item.group_max_gap_seconds // 0) as $groupMaxGapSeconds
                | ($item.group_occurrence_window_seconds // 0) as $groupOccurrenceWindowSeconds
                | ($prevGroup.last_seen_at_epoch // 0) as $previousGroupLastSeen
                | ((($groupMaxGapSeconds > 0) and ($previousGroupLastSeen > 0) and (($now - $previousGroupLastSeen) > $groupMaxGapSeconds))) as $groupGapReset
                | (if has($groupKey) then .[$groupKey] else null end) as $currentGroup
                | (if $currentGroup != null then ($currentGroup.occurrence_timestamps // []) elif $groupGapReset then [] else ($prevGroup.occurrence_timestamps // []) end) as $baseGroupTimestamps
                | (($baseGroupTimestamps + [$now])
                    | map(select(type == "number"))
                    | map(floor)
                    | sort
                    | unique
                    | if $groupOccurrenceWindowSeconds > 0 then map(select(. >= ($now - $groupOccurrenceWindowSeconds))) else . end
                  ) as $groupTimestamps
                | .[$groupKey] = {
                    group_key: $groupKey,
                    member_policy_names: ((((if $currentGroup != null then ($currentGroup.member_policy_names // []) else ($prevGroup.member_policy_names // []) end) + [$item.name]) | unique) | sort),
                    first_seen_at_epoch: (if $currentGroup != null then ($currentGroup.first_seen_at_epoch // $now) elif $groupGapReset then $now else ($prevGroup.first_seen_at_epoch // $now) end),
                    last_seen_at_epoch: $now,
                    occurrences: ($groupTimestamps | length),
                    occurrence_timestamps: $groupTimestamps,
                    max_gap_seconds: $groupMaxGapSeconds,
                    occurrence_window_seconds: $groupOccurrenceWindowSeconds
                  }
              end)
        ) as $matchedGroups
      | (reduce $matchedPolicies[] as $item ({}; .[$item.name] = $item)) as $policyState
      | if (($matchedGroups | keys | length) > 0) then ($policyState + {"__groups": $matchedGroups}) else $policyState end
    ' > "$POLICY_OBSERVATION_STATE_PATH"

  if [[ ! -s "$POLICY_OBSERVATION_STATE_PATH" ]] || [[ "$(jq -r 'keys | map(select(. != "__groups")) | length' "$POLICY_OBSERVATION_STATE_PATH" 2>/dev/null || echo 0)" == "0" ]]; then
    rm -f "$POLICY_OBSERVATION_STATE_PATH"
  fi
}

select_policy_command() {
  local body_file="$1"

  if [[ -z "$NOTIFY_POLICY_JSON" ]]; then
    printf '\n\n\n\n\n\n\n\n\n'
    return 0
  fi

  local policy_candidates_json
  policy_candidates_json="$(build_policy_candidates_json "$body_file")"

  jq -r '
    map(select(.eligible == true and .suppressed != true))
    | sort_by(-(.priority // 0), -(.specificity_score // 0), .rule_index)
    | if length == 0 then ["", "", "", "", "", "", "", "false", "false"] else [.[0].primary_signal_key, .[0].effective_command, .[0].effective_route, .[0].name, (. [0].signal_keys | join(",")), (.[0].occurrences|tostring), (.[0].active_seconds|tostring), (.[0].escalated|tostring), (.[0].group_escalated|tostring)] end
    | .[]' <<<"$policy_candidates_json"
}

build_policy_candidates_json() {
  local body_file="$1"

  if [[ -z "$NOTIFY_POLICY_JSON" ]]; then
    printf '[]\n'
    return 0
  fi

  local observation_state='{}'
  if [[ -f "$POLICY_OBSERVATION_STATE_PATH" ]]; then
    observation_state="$(jq -c '.' "$POLICY_OBSERVATION_STATE_PATH" 2>/dev/null || echo '{}')"
  fi

  jq -c --argjson rules "$NOTIFY_POLICY_JSON" --argjson observed "$observation_state" --argjson now "$NOW_EPOCH" '
    [(.alerts // [])[]
      | {
          signal_key: ((.service // "") + ":" + (.name // "")),
          severity_rank: (if .severity == "critical" then 0 elif .severity == "warn" then 1 else 2 end)
        }
    ] as $alerts
    | ($alerts | map(.signal_key)) as $keys
    | ($observed.__groups // {}) as $groupState
    | [ $rules | to_entries[]
        | .key as $ruleIndex
        | .value as $rule
        | ($rule.matchAll // []) as $matchAll
        | ($rule.matchAny // []) as $matchAny
        | ($rule.matchNone // []) as $matchNone
        | ($rule.suppressedByPolicies // []) as $suppressedByPolicies
        | ($rule.suppressedByGroups // []) as $suppressedByGroups
        | ($rule.name // ($matchAll[0] // $matchAny[0] // $matchNone[0] // "policy")) as $name
        | (($rule.groupKey // "")) as $groupKey
        | ($observed[$name] // {}) as $state
        | ($state.occurrences // 0) as $occurrences
        | ($state.first_seen_at_epoch // $now) as $firstSeen
        | ($now - $firstSeen) as $activeSeconds
        | (($rule.priority // 0) | floor) as $priority
        | (($rule.minOccurrences // 1) | floor) as $minOccurrences
        | (($rule.minActiveSeconds // 0) | floor) as $minActiveSeconds
        | (($rule.groupMinOccurrences // 0) | floor) as $groupMinOccurrences
        | (($rule.groupMinActiveSeconds // 0) | floor) as $groupMinActiveSeconds
        | (($rule.occurrenceWindowSeconds // 0) | floor) as $occurrenceWindowSeconds
        | (($rule.maxGapSeconds // 0) | floor) as $maxGapSeconds
        | (($rule.groupOccurrenceWindowSeconds // ($rule.occurrenceWindowSeconds // 0)) | floor) as $groupOccurrenceWindowSeconds
        | (($rule.groupMaxGapSeconds // ($rule.maxGapSeconds // 0)) | floor) as $groupMaxGapSeconds
        | (($rule.escalateAfterOccurrences // 0) | floor) as $escalateAfterOccurrences
        | (($rule.escalateAfterSeconds // 0) | floor) as $escalateAfterSeconds
        | (($rule.groupEscalateAfterOccurrences // 0) | floor) as $groupEscalateAfterOccurrences
        | (($rule.groupEscalateAfterSeconds // 0) | floor) as $groupEscalateAfterSeconds
        | (($rule.escalationCommand // "")) as $escalationCommand
        | (($rule.escalationRoute // "")) as $escalationRoute
        | (($rule.groupEscalationCommand // "")) as $groupEscalationCommand
        | (($rule.groupEscalationRoute // "")) as $groupEscalationRoute
        | (($groupState[$groupKey] // {}) ) as $group
        | (($group.occurrences // 0)) as $groupOccurrences
        | (($group.first_seen_at_epoch // $now)) as $groupFirstSeen
        | (($now - $groupFirstSeen)) as $groupActiveSeconds
        | select(($rule.command // "") != "")
        | select(([$matchAll[]? as $matchAllKey | select($keys | index($matchAllKey)) | $matchAllKey] | length) == ($matchAll | length))
        | select((($matchAny | length) == 0) or ([ $matchAny[]? as $matchAnyKey | select($keys | index($matchAnyKey)) | $matchAnyKey ] | length) > 0)
        | ([ $matchNone[]? as $matchNoneKey | select($keys | index($matchNoneKey)) | $matchNoneKey ] | unique | sort) as $blockedBySignals
        | {
            name: $name,
            command: $rule.command,
            route: ($rule.route // ("policy:" + $name)),
            rule_index: $ruleIndex,
            priority: $priority,
            group_key: $groupKey,
            primary_signal_key: ($matchAll[0] // $matchAny[0] // ($alerts | sort_by(.severity_rank, .signal_key) | .[0].signal_key // "")),
            signal_keys: ((($matchAll + $matchAny) | unique) | sort),
            match_none: ($matchNone | map(select(type == "string" and length > 0)) | unique | sort),
            specificity_score: (((($matchAll | length) * 100) + (($matchAny | length) * 10) + (($matchNone | length) * 100) + (if ($escalateAfterOccurrences > 0 or $escalateAfterSeconds > 0) then 1 else 0 end))),
            occurrences: $occurrences,
            active_seconds: $activeSeconds,
            min_occurrences: $minOccurrences,
            min_active_seconds: $minActiveSeconds,
            occurrence_window_seconds: $occurrenceWindowSeconds,
            max_gap_seconds: $maxGapSeconds,
            group_occurrences: $groupOccurrences,
            group_active_seconds: $groupActiveSeconds,
            group_min_occurrences: $groupMinOccurrences,
            group_min_active_seconds: $groupMinActiveSeconds,
            group_occurrence_window_seconds: $groupOccurrenceWindowSeconds,
            group_max_gap_seconds: $groupMaxGapSeconds,
            escalate_after_occurrences: $escalateAfterOccurrences,
            escalate_after_seconds: $escalateAfterSeconds,
            group_escalate_after_occurrences: $groupEscalateAfterOccurrences,
            group_escalate_after_seconds: $groupEscalateAfterSeconds,
            escalation_command_configured: (($escalationCommand | length) > 0),
            group_escalation_command_configured: (($groupEscalationCommand | length) > 0),
            escalation_route: (if ($escalationRoute | length) > 0 then $escalationRoute else ((($rule.route // ("policy:" + $name))) + ":escalated") end),
            group_escalation_route: (if ($groupEscalationRoute | length) > 0 then $groupEscalationRoute else ((($rule.route // ("policy:" + $name))) + ":group-escalated") end),
            suppressed_by_policies: ($suppressedByPolicies | map(select(type == "string" and length > 0)) | unique),
            suppressed_by_groups: ($suppressedByGroups | map(select(type == "string" and length > 0)) | unique),
            blocked_by_signal_keys: $blockedBySignals,
            match_none_blocked: (($blockedBySignals | length) > 0),
            group_eligible: (((($groupKey | length) == 0) and true) or (((($groupMinOccurrences <= 0) or ($groupOccurrences >= $groupMinOccurrences)) and (($groupMinActiveSeconds <= 0) or ($groupActiveSeconds >= $groupMinActiveSeconds))))),
            eligible: ((($occurrences >= $minOccurrences) and ($activeSeconds >= $minActiveSeconds)) and (($blockedBySignals | length) == 0) and (((($groupKey | length) == 0) and true) or (((($groupMinOccurrences <= 0) or ($groupOccurrences >= $groupMinOccurrences)) and (($groupMinActiveSeconds <= 0) or ($groupActiveSeconds >= $groupMinActiveSeconds)))))),
            escalated: (((($escalateAfterOccurrences > 0) or ($escalateAfterSeconds > 0))) and (($occurrences >= $minOccurrences) and ($activeSeconds >= $minActiveSeconds)) and (($escalateAfterOccurrences <= 0) or ($occurrences >= $escalateAfterOccurrences)) and (($escalateAfterSeconds <= 0) or ($activeSeconds >= $escalateAfterSeconds))),
            group_escalated: ((((($groupKey | length) > 0) and (($groupEscalateAfterOccurrences > 0) or ($groupEscalateAfterSeconds > 0)))) and (((($groupMinOccurrences <= 0) or ($groupOccurrences >= $groupMinOccurrences)) and (($groupMinActiveSeconds <= 0) or ($groupActiveSeconds >= $groupMinActiveSeconds)))) and (($groupEscalateAfterOccurrences <= 0) or ($groupOccurrences >= $groupEscalateAfterOccurrences)) and (($groupEscalateAfterSeconds <= 0) or ($groupActiveSeconds >= $groupEscalateAfterSeconds))),
            effective_command: (if ((((($groupKey | length) > 0) and (($groupEscalateAfterOccurrences > 0) or ($groupEscalateAfterSeconds > 0)))) and (((($groupMinOccurrences <= 0) or ($groupOccurrences >= $groupMinOccurrences)) and (($groupMinActiveSeconds <= 0) or ($groupActiveSeconds >= $groupMinActiveSeconds)))) and (($groupEscalateAfterOccurrences <= 0) or ($groupOccurrences >= $groupEscalateAfterOccurrences)) and (($groupEscalateAfterSeconds <= 0) or ($groupActiveSeconds >= $groupEscalateAfterSeconds)) and (($groupEscalationCommand | length) > 0)) then $groupEscalationCommand elif (((($escalateAfterOccurrences > 0) or ($escalateAfterSeconds > 0))) and (($occurrences >= $minOccurrences) and ($activeSeconds >= $minActiveSeconds)) and (($escalateAfterOccurrences <= 0) or ($occurrences >= $escalateAfterOccurrences)) and (($escalateAfterSeconds <= 0) or ($activeSeconds >= $escalateAfterSeconds)) and (($escalationCommand | length) > 0)) then $escalationCommand else $rule.command end),
            effective_route: (if ((((($groupKey | length) > 0) and (($groupEscalateAfterOccurrences > 0) or ($groupEscalateAfterSeconds > 0)))) and (((($groupMinOccurrences <= 0) or ($groupOccurrences >= $groupMinOccurrences)) and (($groupMinActiveSeconds <= 0) or ($groupActiveSeconds >= $groupMinActiveSeconds)))) and (($groupEscalateAfterOccurrences <= 0) or ($groupOccurrences >= $groupEscalateAfterOccurrences)) and (($groupEscalateAfterSeconds <= 0) or ($groupActiveSeconds >= $groupEscalateAfterSeconds))) then (if ($groupEscalationRoute | length) > 0 then $groupEscalationRoute else ((($rule.route // ("policy:" + $name))) + ":group-escalated") end) elif (((($escalateAfterOccurrences > 0) or ($escalateAfterSeconds > 0))) and (($occurrences >= $minOccurrences) and ($activeSeconds >= $minActiveSeconds)) and (($escalateAfterOccurrences <= 0) or ($occurrences >= $escalateAfterOccurrences)) and (($escalateAfterSeconds <= 0) or ($activeSeconds >= $escalateAfterSeconds))) then (if ($escalationRoute | length) > 0 then $escalationRoute else ((($rule.route // ("policy:" + $name))) + ":escalated") end) else ($rule.route // ("policy:" + $name)) end)
          }
      ]
    | . as $candidates
    | ($candidates | map(select(.eligible == true) | .name)) as $eligibleNames
    | ($candidates | map(select(.eligible == true and ((.group_key // "") | length) > 0) | .group_key) | unique) as $eligibleGroupKeys
    | [ $candidates[]
        | (.suppressed_by_policies | map(. as $policyName | select($eligibleNames | index($policyName)))) as $matchedSuppressors
        | (.suppressed_by_groups | map(. as $groupName | select($eligibleGroupKeys | index($groupName)))) as $matchedSuppressorGroups
        | . + {
            suppressed: ((($matchedSuppressors | length) + ($matchedSuppressorGroups | length)) > 0),
            suppressed_by: $matchedSuppressors,
            suppressed_by_groups_matched: $matchedSuppressorGroups
          }
      ]
  ' "$body_file"
}

build_policy_selection_trace_json() {
  local policy_candidates_json="$1"
  local selected_policy_name="${2:-}"
  local selected_route="${3:-}"

  jq -c --arg selectedPolicyName "$selected_policy_name" --arg selectedRoute "$selected_route" '
    . as $candidates
    | ($candidates | map(select(.eligible == true and .suppressed != true)) | sort_by(-(.priority // 0), -(.specificity_score // 0), .rule_index)) as $ranked
    | ($candidates
        | sort_by(-(.priority // 0), -(.specificity_score // 0), .rule_index)
        | map(. + {
            selected: (.name == $selectedPolicyName and .effective_route == $selectedRoute),
            decision_summary: (
              if .match_none_blocked then "blocked_by_match_none"
              elif (.group_eligible | not) then "blocked_by_group_threshold"
              elif (.occurrences < .min_occurrences) then "waiting_for_policy_occurrences"
              elif (.active_seconds < .min_active_seconds) then "waiting_for_policy_active_seconds"
              elif ((.suppressed_by_groups_matched // []) | length) > 0 then "suppressed_by_group"
              elif ((.suppressed_by // []) | length) > 0 then "suppressed_by_policy"
              elif (.name == $selectedPolicyName and .effective_route == $selectedRoute and .group_escalated == true) then "selected_group_escalation"
              elif (.name == $selectedPolicyName and .effective_route == $selectedRoute and .escalated == true) then "selected_policy_escalation"
              elif (.name == $selectedPolicyName and .effective_route == $selectedRoute) then "selected"
              elif .eligible then "eligible_not_selected"
              else "ineligible"
              end
            ),
            decision_detail: (
              if .match_none_blocked then ("blocked by matchNone: " + (((.blocked_by_signal_keys // []) | join(", "))))
              elif (.group_eligible | not) then ("waiting for group thresholds, group=" + (.group_key // "") + ", occurrences=" + ((.group_occurrences // 0) | tostring) + "/" + ((.group_min_occurrences // 0) | tostring) + ", active_seconds=" + ((.group_active_seconds // 0) | tostring) + "/" + ((.group_min_active_seconds // 0) | tostring))
              elif (.occurrences < .min_occurrences) then ("waiting for policy occurrences, occurrences=" + ((.occurrences // 0) | tostring) + "/" + ((.min_occurrences // 0) | tostring))
              elif (.active_seconds < .min_active_seconds) then ("waiting for policy active seconds, active_seconds=" + ((.active_seconds // 0) | tostring) + "/" + ((.min_active_seconds // 0) | tostring))
              elif ((.suppressed_by_groups_matched // []) | length) > 0 then ("suppressed by groups: " + ((.suppressed_by_groups_matched // []) | join(", ")))
              elif ((.suppressed_by // []) | length) > 0 then ("suppressed by policies: " + ((.suppressed_by // []) | join(", ")))
              elif (.name == $selectedPolicyName and .effective_route == $selectedRoute and .group_escalated == true) then ("selected via group escalation, route=" + (.effective_route // ""))
              elif (.name == $selectedPolicyName and .effective_route == $selectedRoute and .escalated == true) then ("selected via policy escalation, route=" + (.effective_route // ""))
              elif (.name == $selectedPolicyName and .effective_route == $selectedRoute) then ("selected by priority=" + ((.priority // 0) | tostring) + ", specificity_score=" + ((.specificity_score // 0) | tostring) + ", rule_index=" + ((.rule_index // 0) | tostring))
              elif .eligible then ("eligible but outranked by selected candidate, priority=" + ((.priority // 0) | tostring) + ", specificity_score=" + ((.specificity_score // 0) | tostring) + ", rule_index=" + ((.rule_index // 0) | tostring))
              else "not selected"
              end
            )
          }
          | {
              name,
              group_key,
              effective_route,
              priority,
              specificity_score,
              rule_index,
              eligible,
              suppressed,
              suppressed_by,
              suppressed_by_groups_matched,
              escalated,
              group_escalated,
              selected,
              decision_summary,
              decision_detail
            })
      ) as $explained
    | ($explained | map(select(.selected)) | .[0]) as $selected
    | ($explained | map(select((.selected | not) and (.decision_summary != "ineligible"))) | .[0:3]) as $noteworthy
    | (
        (if $selected == null then ["no eligible policy candidate selected"]
         elif $selected.group_escalated == true then [("selected " + $selected.name + " via group escalation to " + ($selected.effective_route // ""))]
         elif $selected.escalated == true then [("selected " + $selected.name + " via policy escalation to " + ($selected.effective_route // ""))]
         else [("selected " + $selected.name + " via priority/specificity ordering to " + ($selected.effective_route // ""))]
         end)
        + ($noteworthy | map(
            if .decision_summary == "eligible_not_selected" then (.name + " was eligible but lost on priority/specificity ordering")
            elif .decision_summary == "suppressed_by_policy" then (.name + " was suppressed by policy " + ((.suppressed_by // []) | join(", ")))
            elif .decision_summary == "suppressed_by_group" then (.name + " was suppressed by group " + ((.suppressed_by_groups_matched // []) | join(", ")))
            elif .decision_summary == "blocked_by_match_none" then (.name + " was blocked by matchNone " + ((.decision_detail // "") | sub("^blocked by matchNone: "; "")))
            elif .decision_summary == "blocked_by_group_threshold" then (.name + " is waiting on group thresholds")
            elif .decision_summary == "waiting_for_policy_occurrences" then (.name + " is waiting for more policy occurrences")
            elif .decision_summary == "waiting_for_policy_active_seconds" then (.name + " is waiting for more policy active seconds")
            else (.name + " -> " + .decision_summary)
            end
          ))
      ) as $summaryLines
    | {
        selected_policy_name: (if $selectedPolicyName == "" then null else $selectedPolicyName end),
        selected_route: (if $selectedRoute == "" then null else $selectedRoute end),
        eligible_candidate_count: ($ranked | length),
        winning_candidate: (if ($ranked | length) == 0 then null else ($ranked[0] | {
          name,
          effective_route,
          priority,
          specificity_score,
          rule_index,
          escalated,
          group_escalated
        }) end),
        summary_lines: $summaryLines,
        summary_text: ($summaryLines | join("; ")),
        candidates: $explained
      }
  ' <<<"$policy_candidates_json"
}

build_notify_policy_summary_short_text() {
  local trace_json="$1"
  local notify_reason="$2"
  local notify_severity="$3"
  local selected_policy_name="${4:-}"
  local selected_route="${5:-}"
  local previous_severity="${6:-unknown}"
  local policy_escalated="${7:-false}"
  local policy_group_escalated="${8:-false}"

  local trace_summary=""
  trace_summary="$(jq -r '.summary_text // ""' <<< "$trace_json" 2>/dev/null || true)"

  case "$notify_reason" in
    recovered_below_threshold)
      if [[ -n "$selected_policy_name" ]]; then
        printf 'recovered below threshold from %s, previous policy %s\n' "$previous_severity" "$selected_policy_name"
      else
        printf 'recovered below threshold from %s\n' "$previous_severity"
      fi
      return 0
      ;;
    notify_disabled)
      printf 'notifications disabled for current state\n'
      return 0
      ;;
    command_missing)
      if [[ -n "$selected_policy_name" && -n "$selected_route" ]]; then
        printf 'matched %s -> %s but no notify command is configured\n' "$selected_policy_name" "$selected_route"
      elif [[ -n "$trace_summary" ]]; then
        printf 'no notify command configured, %s\n' "$trace_summary"
      else
        printf 'no notify command configured\n'
      fi
      return 0
      ;;
    threshold_not_met)
      if [[ -n "$trace_summary" ]]; then
        printf 'below notify threshold, %s\n' "$trace_summary"
      else
        printf 'below notify threshold\n'
      fi
      return 0
      ;;
  esac

  local prefix="signal state"
  case "$notify_severity" in
    critical)
      prefix="critical incident"
      ;;
    warn)
      prefix="warning"
      ;;
    ok)
      prefix="recovered state"
      ;;
  esac

  local lead="$prefix"
  case "$notify_reason" in
    reminder_interval_elapsed)
      lead="$prefix reminder"
      ;;
    routing_changed)
      lead="$prefix routing update"
      ;;
    state_changed)
      lead="$prefix new state"
      ;;
    always)
      lead="$prefix forced send"
      ;;
  esac

  local action="routed to"
  if [[ "$policy_group_escalated" == "true" ]]; then
    action="group-escalated to"
  elif [[ "$policy_escalated" == "true" ]]; then
    action="policy-escalated to"
  fi

  if [[ -n "$selected_policy_name" && -n "$selected_route" ]]; then
    printf '%s: %s %s %s\n' "$lead" "$selected_policy_name" "$action" "$selected_route"
  elif [[ -n "$trace_summary" ]]; then
    printf '%s: %s\n' "$lead" "$trace_summary"
  else
    printf '%s\n' "$lead"
  fi
}

build_notify_policy_summary_ultra_short_text() {
  local notify_reason="$1"
  local notify_severity="$2"
  local selected_policy_name="${3:-}"
  local selected_route="${4:-}"
  local previous_severity="${5:-unknown}"
  local policy_escalated="${6:-false}"
  local policy_group_escalated="${7:-false}"

  case "$notify_reason" in
    recovered_below_threshold)
      printf 'recovered from %s\n' "$previous_severity"
      return 0
      ;;
    notify_disabled)
      printf 'notifications disabled\n'
      return 0
      ;;
    command_missing)
      printf 'notify command missing\n'
      return 0
      ;;
    threshold_not_met)
      printf 'below notify threshold\n'
      return 0
      ;;
  esac

  local sev="$notify_severity"
  case "$sev" in
    critical)
      sev="critical"
      ;;
    warn)
      sev="warning"
      ;;
    ok)
      sev="recovered"
      ;;
    *)
      sev="state"
      ;;
  esac

  local marker="route"
  if [[ "$policy_group_escalated" == "true" ]]; then
    marker="group-escalation"
  elif [[ "$policy_escalated" == "true" ]]; then
    marker="policy-escalation"
  fi

  if [[ -n "$selected_policy_name" && -n "$selected_route" ]]; then
    printf '%s: %s -> %s (%s)\n' "$sev" "$selected_policy_name" "$selected_route" "$marker"
  elif [[ -n "$selected_policy_name" ]]; then
    printf '%s: %s\n' "$sev" "$selected_policy_name"
  else
    printf '%s\n' "$sev"
  fi
}

build_notify_policy_summary_full_text() {
  local trace_json="$1"
  local short_text="$2"
  local trace_summary=""
  trace_summary="$(jq -r '.summary_text // ""' <<< "$trace_json" 2>/dev/null || true)"

  if [[ -z "$trace_summary" || "$trace_summary" == "$short_text" ]]; then
    printf '%s\n' "$short_text"
  else
    printf '%s; %s\n' "$short_text" "$trace_summary"
  fi
}

select_signal_specific_command() {
  local body_file="$1"

  if [[ -z "$NOTIFY_SIGNAL_COMMANDS_JSON" ]]; then
    printf '\n\n\n'
    return 0
  fi

  jq -r --argjson routes "$NOTIFY_SIGNAL_COMMANDS_JSON" '
    [(.alerts // [])[]
      | . + {
          signal_key: ((.service // "") + ":" + (.name // "")),
          severity_rank: (if .severity == "critical" then 0 elif .severity == "warn" then 1 else 2 end)
        }
      | select(($routes[.signal_key] // null) != null)
      | select(($routes[.signal_key] | type) == "string")
      | select(($routes[.signal_key] | length) > 0)
    ]
    | sort_by(.severity_rank, .service, .name)
    | if length == 0 then ["", "", ""] else [.[0].signal_key, $routes[.[0].signal_key], ("signal:" + .[0].signal_key)] end
    | .[]' "$body_file"
}

write_notify_state() {
  local state_json="$1"
  local first_epoch="$2"
  local last_epoch="$3"
  local seen_epoch="$4"
  local notify_count="$5"
  local last_reason="$6"
  local last_severity="$7"
  local last_route="$8"
  local last_policy_name="$9"
  local last_signal_key="${10}"
  local last_policy_occurrences="${11}"
  local last_policy_active_seconds="${12}"

  jq -n \
    --argjson state "$state_json" \
    --argjson first_epoch "$first_epoch" \
    --argjson last_epoch "$last_epoch" \
    --argjson seen_epoch "$seen_epoch" \
    --argjson notify_count "$notify_count" \
    --arg first_iso "$(epoch_to_iso "$first_epoch")" \
    --arg last_iso "$(epoch_to_iso "$last_epoch")" \
    --arg seen_iso "$(epoch_to_iso "$seen_epoch")" \
    --arg last_reason "$last_reason" \
    --arg last_severity "$last_severity" \
    --arg last_route "$last_route" \
    --arg last_policy_name "$last_policy_name" \
    --arg last_signal_key "$last_signal_key" \
    --argjson last_policy_occurrences "$last_policy_occurrences" \
    --argjson last_policy_active_seconds "$last_policy_active_seconds" \
    --argjson reminder_secs "$NOTIFY_REMINDER_SECS" \
    '{
      state: $state,
      first_notified_at_epoch: $first_epoch,
      first_notified_at: $first_iso,
      last_notified_at_epoch: $last_epoch,
      last_notified_at: $last_iso,
      last_seen_at_epoch: $seen_epoch,
      last_seen_at: $seen_iso,
      notify_count: $notify_count,
      last_notify_reason: $last_reason,
      last_notify_severity: $last_severity,
      last_notify_route: $last_route,
      last_notify_policy_name: $last_policy_name,
      last_notify_signal_key: $last_signal_key,
      last_notify_policy_occurrences: $last_policy_occurrences,
      last_notify_policy_active_seconds: $last_policy_active_seconds,
      reminder_seconds: $reminder_secs
    }' > "$NOTIFY_STATE_PATH"
}

while (($#)); do
  case "$1" in
    --compact)
      OUTPUT_MODE="compact"
      ;;
    --pretty)
      OUTPUT_MODE="pretty"
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "unknown arg: $1" >&2
      usage >&2
      exit 64
      ;;
  esac
  shift
done

if [[ "$NOTIFY_CHANGES_ONLY" != "0" && "$NOTIFY_CHANGES_ONLY" != "1" ]]; then
  echo "invalid OPERATOR_SIGNAL_NOTIFY_CHANGES_ONLY: $NOTIFY_CHANGES_ONLY" >&2
  exit 64
fi

if [[ "$NOTIFY_RECOVERY" != "0" && "$NOTIFY_RECOVERY" != "1" ]]; then
  echo "invalid OPERATOR_SIGNAL_NOTIFY_RECOVERY: $NOTIFY_RECOVERY" >&2
  exit 64
fi

if ! [[ "$NOTIFY_REMINDER_SECS" =~ ^[0-9]+$ ]]; then
  echo "invalid OPERATOR_SIGNAL_NOTIFY_REMINDER_SECS: $NOTIFY_REMINDER_SECS" >&2
  exit 64
fi

if [[ -n "$NOTIFY_SIGNAL_COMMANDS_JSON" ]] && ! jq -e 'type == "object" and all(values[]?; type == "string")' >/dev/null 2>&1 <<<"$NOTIFY_SIGNAL_COMMANDS_JSON"; then
  echo "invalid OPERATOR_SIGNAL_NOTIFY_SIGNAL_COMMANDS_JSON: must be a json object of string commands" >&2
  exit 64
fi

if [[ -n "$NOTIFY_POLICY_JSON" ]] && ! jq -e 'type == "array" and all(.[]?; type == "object" and ((.command // "") | type == "string") and ((.command // "") | length > 0) and ((.escalationCommand // "") | type == "string") and ((.groupEscalationCommand // "") | type == "string") and ((.name // "") | type == "string") and ((.route // "") | type == "string") and ((.escalationRoute // "") | type == "string") and ((.groupEscalationRoute // "") | type == "string") and ((.groupKey // "") | type == "string") and (((.priority // 0) | type) == "number") and (((.minOccurrences // 1) | type) == "number") and (((.minOccurrences // 1) >= 0)) and (((.minActiveSeconds // 0) | type) == "number") and (((.minActiveSeconds // 0) >= 0)) and (((.groupMinOccurrences // 0) | type) == "number") and (((.groupMinOccurrences // 0) >= 0)) and (((.groupMinActiveSeconds // 0) | type) == "number") and (((.groupMinActiveSeconds // 0) >= 0)) and (((.occurrenceWindowSeconds // 0) | type) == "number") and (((.occurrenceWindowSeconds // 0) >= 0)) and (((.maxGapSeconds // 0) | type) == "number") and (((.maxGapSeconds // 0) >= 0)) and (((.groupOccurrenceWindowSeconds // (.occurrenceWindowSeconds // 0)) | type) == "number") and (((.groupOccurrenceWindowSeconds // (.occurrenceWindowSeconds // 0)) >= 0)) and (((.groupMaxGapSeconds // (.maxGapSeconds // 0)) | type) == "number") and (((.groupMaxGapSeconds // (.maxGapSeconds // 0)) >= 0)) and (((.escalateAfterOccurrences // 0) | type) == "number") and (((.escalateAfterOccurrences // 0) >= 0)) and (((.escalateAfterSeconds // 0) | type) == "number") and (((.escalateAfterSeconds // 0) >= 0)) and (((.groupEscalateAfterOccurrences // 0) | type) == "number") and (((.groupEscalateAfterOccurrences // 0) >= 0)) and (((.groupEscalateAfterSeconds // 0) | type) == "number") and (((.groupEscalateAfterSeconds // 0) >= 0)) and ((.matchAll // []) | type == "array") and ((.matchAny // []) | type == "array") and ((.matchNone // []) | type == "array") and ((.suppressedByPolicies // []) | type == "array") and ((.suppressedByGroups // []) | type == "array") and all((.matchAll // [])[]?; type == "string") and all((.matchAny // [])[]?; type == "string") and all((.matchNone // [])[]?; type == "string") and all((.suppressedByPolicies // [])[]?; type == "string") and all((.suppressedByGroups // [])[]?; type == "string") )' >/dev/null 2>&1 <<<"$NOTIFY_POLICY_JSON"; then
  echo "invalid OPERATOR_SIGNAL_NOTIFY_POLICY_JSON: must be a json array of rule objects" >&2
  exit 64
fi

mkdir -p "$STATE_DIR"
TMP_RESULT="$(mktemp "$STATE_DIR/check-result.XXXXXX.json")"
cleanup() {
  rm -f "$TMP_RESULT"
}
trap cleanup EXIT

set +e
"$CHECK_SCRIPT" --compact > "$TMP_RESULT"
CHECK_EXIT=$?
set -e

if ! jq empty "$TMP_RESULT" >/dev/null 2>&1; then
  echo "check script did not produce valid json: $TMP_RESULT" >&2
  exit 70
fi

CHECKED_AT="$(jq -r '.checked_at // empty' "$TMP_RESULT")"
if [[ -z "$CHECKED_AT" ]]; then
  CHECKED_AT="$(date -Iseconds)"
fi
NOW_EPOCH="$(iso_to_epoch "$CHECKED_AT")"
SAFE_STAMP="$(printf '%s' "$CHECKED_AT" | tr ':+' '__' | tr -d 'T-' )"
LAST_PATH="$STATE_DIR/last.json"
STAMPED_PATH="$STATE_DIR/result-${SAFE_STAMP}.json"
STATUS_PATH="$STATE_DIR/last.status"
NOTIFY_STATE_PATH="$STATE_DIR/last-notify-state.json"
POLICY_OBSERVATION_STATE_PATH="$STATE_DIR/policy-observations.json"

cp "$TMP_RESULT" "$LAST_PATH"
cp "$TMP_RESULT" "$STAMPED_PATH"
printf '%s\n' "$CHECK_EXIT" > "$STATUS_PATH"

update_policy_observation_state "$LAST_PATH"

CURRENT_NOTIFY_STATE="$(jq -c --argjson check_exit "$CHECK_EXIT" '
  {
    overall: (.overall // "unknown"),
    exit_code: (.exit_code // $check_exit),
    alerts: ((.alerts // [])
      | map({
          service: (.service // ""),
          name: (.name // ""),
          severity: (.severity // ""),
          value: (.value // null),
          threshold: (.threshold // null),
          message: (.message // "")
        })
      | sort_by(.service, .name, .severity, (.value // 0), (.threshold // 0), .message)),
    source_errors: (
      (.sources // {})
      | to_entries
      | map(select((.value.fetch.ok // false) != true)
          | {
              source: .key,
              ok: (.value.fetch.ok // false),
              http_code: (.value.fetch.http_code // ""),
              error: (.value.fetch.error // "")
            })
      | sort_by(.source, .http_code, .error))
  }' "$LAST_PATH")"
CURRENT_SEVERITY="$(severity_from_exit "$CHECK_EXIT")"
POLICY_CANDIDATES_JSON="$(build_policy_candidates_json "$LAST_PATH")"
POLICY_SELECTION_TRACE_JSON='{"selected_policy_name":null,"selected_route":null,"eligible_candidate_count":0,"winning_candidate":null,"summary_lines":[],"summary_text":"","candidates":[]}'
POLICY_SUMMARY_TEXT=""
POLICY_SUMMARY_ULTRA_SHORT_TEXT=""
POLICY_SUMMARY_FULL_TEXT=""

PREVIOUS_NOTIFY_STATE=""
PREVIOUS_FIRST_NOTIFIED_AT_EPOCH=0
PREVIOUS_LAST_NOTIFIED_AT_EPOCH=0
PREVIOUS_NOTIFY_COUNT=0
PREVIOUS_LAST_NOTIFY_REASON=""
PREVIOUS_LAST_NOTIFY_SEVERITY="ok"
PREVIOUS_LAST_NOTIFY_ROUTE="missing"
PREVIOUS_LAST_NOTIFY_POLICY_NAME=""
PREVIOUS_LAST_NOTIFY_SIGNAL_KEY=""
PREVIOUS_LAST_NOTIFY_POLICY_OCCURRENCES=0
PREVIOUS_LAST_NOTIFY_POLICY_ACTIVE_SECONDS=0
PREVIOUS_STATE_EXIT_CODE=0
PREVIOUS_STATE_SEVERITY="ok"
if [[ -f "$NOTIFY_STATE_PATH" ]]; then
  PREVIOUS_NOTIFY_STATE="$(jq -c '.state // {}' "$NOTIFY_STATE_PATH" 2>/dev/null || true)"
  PREVIOUS_FIRST_NOTIFIED_AT_EPOCH="$(jq -r '.first_notified_at_epoch // 0' "$NOTIFY_STATE_PATH" 2>/dev/null || echo 0)"
  PREVIOUS_LAST_NOTIFIED_AT_EPOCH="$(jq -r '.last_notified_at_epoch // 0' "$NOTIFY_STATE_PATH" 2>/dev/null || echo 0)"
  PREVIOUS_NOTIFY_COUNT="$(jq -r '.notify_count // 0' "$NOTIFY_STATE_PATH" 2>/dev/null || echo 0)"
  PREVIOUS_LAST_NOTIFY_REASON="$(jq -r '.last_notify_reason // ""' "$NOTIFY_STATE_PATH" 2>/dev/null || true)"
  PREVIOUS_LAST_NOTIFY_SEVERITY="$(jq -r '.last_notify_severity // "ok"' "$NOTIFY_STATE_PATH" 2>/dev/null || echo ok)"
  PREVIOUS_LAST_NOTIFY_ROUTE="$(jq -r '.last_notify_route // "missing"' "$NOTIFY_STATE_PATH" 2>/dev/null || echo missing)"
  PREVIOUS_LAST_NOTIFY_POLICY_NAME="$(jq -r '.last_notify_policy_name // ""' "$NOTIFY_STATE_PATH" 2>/dev/null || true)"
  PREVIOUS_LAST_NOTIFY_SIGNAL_KEY="$(jq -r '.last_notify_signal_key // ""' "$NOTIFY_STATE_PATH" 2>/dev/null || true)"
  PREVIOUS_LAST_NOTIFY_POLICY_OCCURRENCES="$(jq -r '.last_notify_policy_occurrences // 0' "$NOTIFY_STATE_PATH" 2>/dev/null || echo 0)"
  PREVIOUS_LAST_NOTIFY_POLICY_ACTIVE_SECONDS="$(jq -r '.last_notify_policy_active_seconds // 0' "$NOTIFY_STATE_PATH" 2>/dev/null || echo 0)"
  PREVIOUS_STATE_EXIT_CODE="$(jq -r '.state.exit_code // 0' "$NOTIFY_STATE_PATH" 2>/dev/null || echo 0)"
  PREVIOUS_STATE_SEVERITY="$(severity_from_exit "$PREVIOUS_STATE_EXIT_CODE")"
fi

notify_eligible=false
notify_sent=false
NOTIFY_REASON="threshold_not_met"
SELECTED_NOTIFY_COMMAND=""
SELECTED_NOTIFY_ROUTE="missing"
SELECTED_NOTIFY_POLICY_NAME=""
SELECTED_NOTIFY_POLICY_ESCALATED=false
SELECTED_NOTIFY_POLICY_GROUP_ESCALATED=false
SELECTED_NOTIFY_SIGNAL_KEY=""
SELECTED_NOTIFY_SIGNAL_KEYS=""
SELECTED_NOTIFY_POLICY_OCCURRENCES=0
SELECTED_NOTIFY_POLICY_ACTIVE_SECONDS=0
SELECTED_NOTIFY_SEVERITY="$CURRENT_SEVERITY"
SELECTED_NOTIFY_PREVIOUS_SEVERITY="$PREVIOUS_STATE_SEVERITY"
SELECTED_NOTIFY_PREVIOUS_SIGNAL_KEY="$PREVIOUS_LAST_NOTIFY_SIGNAL_KEY"

if ! threshold_met_for_mode "$NOTIFY_ON" "$CHECK_EXIT"; then
  if [[ "$NOTIFY_ON" == "never" ]]; then
    NOTIFY_REASON="notify_disabled"
  fi

  if [[ "$NOTIFY_RECOVERY" == "1" && -n "$PREVIOUS_NOTIFY_STATE" && "$PREVIOUS_STATE_SEVERITY" != "ok" && "$NOTIFY_ON" != "never" ]]; then
    notify_eligible=true
    NOTIFY_REASON="recovered_below_threshold"
    SELECTED_NOTIFY_SEVERITY="$CURRENT_SEVERITY"
    SELECTED_NOTIFY_POLICY_NAME="$PREVIOUS_LAST_NOTIFY_POLICY_NAME"
    SELECTED_NOTIFY_SIGNAL_KEY="$PREVIOUS_LAST_NOTIFY_SIGNAL_KEY"
    SELECTED_NOTIFY_SIGNAL_KEYS="$PREVIOUS_LAST_NOTIFY_SIGNAL_KEY"
    SELECTED_NOTIFY_POLICY_OCCURRENCES="$PREVIOUS_LAST_NOTIFY_POLICY_OCCURRENCES"
    SELECTED_NOTIFY_POLICY_ACTIVE_SECONDS="$PREVIOUS_LAST_NOTIFY_POLICY_ACTIVE_SECONDS"
    mapfile -t route_info < <(select_notify_command "ok" "$PREVIOUS_STATE_SEVERITY")
    SELECTED_NOTIFY_COMMAND="${route_info[0]:-}"
    SELECTED_NOTIFY_ROUTE="${route_info[1]:-missing}"
  else
    rm -f "$NOTIFY_STATE_PATH"
  fi
else
  notify_eligible=true
  case "$NOTIFY_ON" in
    always)
      NOTIFY_REASON="always"
      ;;
    *)
      NOTIFY_REASON="threshold_met"
      ;;
  esac

  mapfile -t route_info < <(select_notify_command "$CURRENT_SEVERITY")
  SELECTED_NOTIFY_COMMAND="${route_info[0]:-}"
  SELECTED_NOTIFY_ROUTE="${route_info[1]:-missing}"

  mapfile -t policy_route_info < <(select_policy_command "$LAST_PATH")
  if [[ -n "${policy_route_info[0]:-}" && -n "${policy_route_info[1]:-}" ]]; then
    SELECTED_NOTIFY_SIGNAL_KEY="${policy_route_info[0]}"
    SELECTED_NOTIFY_COMMAND="${policy_route_info[1]}"
    SELECTED_NOTIFY_ROUTE="${policy_route_info[2]:-policy}"
    SELECTED_NOTIFY_POLICY_NAME="${policy_route_info[3]:-}"
    SELECTED_NOTIFY_SIGNAL_KEYS="${policy_route_info[4]:-${policy_route_info[0]}}"
    SELECTED_NOTIFY_POLICY_OCCURRENCES="${policy_route_info[5]:-0}"
    SELECTED_NOTIFY_POLICY_ACTIVE_SECONDS="${policy_route_info[6]:-0}"
    SELECTED_NOTIFY_POLICY_ESCALATED="${policy_route_info[7]:-false}"
    SELECTED_NOTIFY_POLICY_GROUP_ESCALATED="${policy_route_info[8]:-false}"
  else
    mapfile -t signal_route_info < <(select_signal_specific_command "$LAST_PATH")
    if [[ -n "${signal_route_info[0]:-}" && -n "${signal_route_info[1]:-}" ]]; then
      SELECTED_NOTIFY_SIGNAL_KEY="${signal_route_info[0]}"
      SELECTED_NOTIFY_COMMAND="${signal_route_info[1]}"
      SELECTED_NOTIFY_ROUTE="${signal_route_info[2]:-signal:${signal_route_info[0]}}"
      SELECTED_NOTIFY_SIGNAL_KEYS="${signal_route_info[0]}"
    fi
  fi

  if [[ "$notify_eligible" == true && "$NOTIFY_CHANGES_ONLY" == "1" && "$NOTIFY_ON" != "always" ]]; then
    if [[ -n "$PREVIOUS_NOTIFY_STATE" && "$PREVIOUS_NOTIFY_STATE" == "$CURRENT_NOTIFY_STATE" ]]; then
      if [[ "$SELECTED_NOTIFY_ROUTE" != "$PREVIOUS_LAST_NOTIFY_ROUTE" || "$SELECTED_NOTIFY_POLICY_NAME" != "$PREVIOUS_LAST_NOTIFY_POLICY_NAME" || "$SELECTED_NOTIFY_SIGNAL_KEY" != "$PREVIOUS_LAST_NOTIFY_SIGNAL_KEY" ]]; then
        NOTIFY_REASON="routing_changed"
      elif [[ "$NOTIFY_REMINDER_SECS" -gt 0 ]] && [[ $(( NOW_EPOCH - PREVIOUS_LAST_NOTIFIED_AT_EPOCH )) -ge "$NOTIFY_REMINDER_SECS" ]]; then
        NOTIFY_REASON="reminder_interval_elapsed"
      else
        notify_eligible=false
        NOTIFY_REASON="unchanged_suppressed"
        if [[ -f "$NOTIFY_STATE_PATH" ]]; then
          write_notify_state \
            "$CURRENT_NOTIFY_STATE" \
            "$PREVIOUS_FIRST_NOTIFIED_AT_EPOCH" \
            "$PREVIOUS_LAST_NOTIFIED_AT_EPOCH" \
            "$NOW_EPOCH" \
            "$PREVIOUS_NOTIFY_COUNT" \
            "$PREVIOUS_LAST_NOTIFY_REASON" \
            "$PREVIOUS_LAST_NOTIFY_SEVERITY" \
            "$PREVIOUS_LAST_NOTIFY_ROUTE" \
            "$PREVIOUS_LAST_NOTIFY_POLICY_NAME" \
            "$PREVIOUS_LAST_NOTIFY_SIGNAL_KEY" \
            "$PREVIOUS_LAST_NOTIFY_POLICY_OCCURRENCES" \
            "$PREVIOUS_LAST_NOTIFY_POLICY_ACTIVE_SECONDS"
        fi
      fi
    else
      NOTIFY_REASON="state_changed"
    fi
  fi
fi

POLICY_SELECTION_TRACE_JSON="$(build_policy_selection_trace_json "$POLICY_CANDIDATES_JSON" "$SELECTED_NOTIFY_POLICY_NAME" "$SELECTED_NOTIFY_ROUTE")"

if [[ "$notify_eligible" == true && -z "$SELECTED_NOTIFY_COMMAND" ]]; then
  notify_eligible=false
  NOTIFY_REASON="command_missing"
  SELECTED_NOTIFY_ROUTE="missing"
  if ! threshold_met_for_mode "$NOTIFY_ON" "$CHECK_EXIT"; then
    rm -f "$NOTIFY_STATE_PATH"
  fi
fi

POLICY_SUMMARY_TEXT="$(build_notify_policy_summary_short_text "$POLICY_SELECTION_TRACE_JSON" "$NOTIFY_REASON" "$SELECTED_NOTIFY_SEVERITY" "$SELECTED_NOTIFY_POLICY_NAME" "$SELECTED_NOTIFY_ROUTE" "$SELECTED_NOTIFY_PREVIOUS_SEVERITY" "$SELECTED_NOTIFY_POLICY_ESCALATED" "$SELECTED_NOTIFY_POLICY_GROUP_ESCALATED")"
POLICY_SUMMARY_ULTRA_SHORT_TEXT="$(build_notify_policy_summary_ultra_short_text "$NOTIFY_REASON" "$SELECTED_NOTIFY_SEVERITY" "$SELECTED_NOTIFY_POLICY_NAME" "$SELECTED_NOTIFY_ROUTE" "$SELECTED_NOTIFY_PREVIOUS_SEVERITY" "$SELECTED_NOTIFY_POLICY_ESCALATED" "$SELECTED_NOTIFY_POLICY_GROUP_ESCALATED")"
POLICY_SUMMARY_FULL_TEXT="$(build_notify_policy_summary_full_text "$POLICY_SELECTION_TRACE_JSON" "$POLICY_SUMMARY_TEXT")"

if [[ "$notify_eligible" == true ]]; then
  OPERATOR_SIGNAL_OVERALL="$(jq -r '.overall // "unknown"' "$LAST_PATH")" \
  OPERATOR_SIGNAL_EXIT_CODE="$CHECK_EXIT" \
  OPERATOR_SIGNAL_RESULT_PATH="$LAST_PATH" \
  OPERATOR_SIGNAL_NOTIFY_REASON="$NOTIFY_REASON" \
  OPERATOR_SIGNAL_NOTIFY_SEVERITY="$SELECTED_NOTIFY_SEVERITY" \
  OPERATOR_SIGNAL_NOTIFY_ROUTE="$SELECTED_NOTIFY_ROUTE" \
  OPERATOR_SIGNAL_NOTIFY_POLICY_NAME="$SELECTED_NOTIFY_POLICY_NAME" \
  OPERATOR_SIGNAL_NOTIFY_POLICY_SUMMARY="$POLICY_SUMMARY_TEXT" \
  OPERATOR_SIGNAL_NOTIFY_POLICY_SUMMARY_ULTRA_SHORT="$POLICY_SUMMARY_ULTRA_SHORT_TEXT" \
  OPERATOR_SIGNAL_NOTIFY_POLICY_SUMMARY_FULL="$POLICY_SUMMARY_FULL_TEXT" \
  OPERATOR_SIGNAL_NOTIFY_POLICY_SUMMARY_LEVEL="${OPERATOR_SIGNAL_NOTIFY_POLICY_SUMMARY_LEVEL:-short}" \
  OPERATOR_SIGNAL_NOTIFY_POLICY_ESCALATED="$SELECTED_NOTIFY_POLICY_ESCALATED" \
  OPERATOR_SIGNAL_NOTIFY_POLICY_GROUP_ESCALATED="$SELECTED_NOTIFY_POLICY_GROUP_ESCALATED" \
  OPERATOR_SIGNAL_NOTIFY_POLICY_OCCURRENCES="$SELECTED_NOTIFY_POLICY_OCCURRENCES" \
  OPERATOR_SIGNAL_NOTIFY_POLICY_ACTIVE_SECONDS="$SELECTED_NOTIFY_POLICY_ACTIVE_SECONDS" \
  OPERATOR_SIGNAL_NOTIFY_SIGNAL_KEY="$SELECTED_NOTIFY_SIGNAL_KEY" \
  OPERATOR_SIGNAL_NOTIFY_SIGNAL_KEYS="$SELECTED_NOTIFY_SIGNAL_KEYS" \
  OPERATOR_SIGNAL_NOTIFY_PREVIOUS_SEVERITY="$SELECTED_NOTIFY_PREVIOUS_SEVERITY" \
  OPERATOR_SIGNAL_NOTIFY_PREVIOUS_SIGNAL_KEY="$SELECTED_NOTIFY_PREVIOUS_SIGNAL_KEY" \
  bash -lc "$SELECTED_NOTIFY_COMMAND" < "$LAST_PATH"

  notify_sent=true

  if threshold_met_for_mode "$NOTIFY_ON" "$CHECK_EXIT"; then
    if [[ -n "$PREVIOUS_NOTIFY_STATE" && "$PREVIOUS_NOTIFY_STATE" == "$CURRENT_NOTIFY_STATE" ]]; then
      write_notify_state \
        "$CURRENT_NOTIFY_STATE" \
        "$PREVIOUS_FIRST_NOTIFIED_AT_EPOCH" \
        "$NOW_EPOCH" \
        "$NOW_EPOCH" \
        "$((PREVIOUS_NOTIFY_COUNT + 1))" \
        "$NOTIFY_REASON" \
        "$SELECTED_NOTIFY_SEVERITY" \
        "$SELECTED_NOTIFY_ROUTE" \
        "$SELECTED_NOTIFY_POLICY_NAME" \
        "$SELECTED_NOTIFY_SIGNAL_KEY" \
        "$SELECTED_NOTIFY_POLICY_OCCURRENCES" \
        "$SELECTED_NOTIFY_POLICY_ACTIVE_SECONDS"
    else
      write_notify_state \
        "$CURRENT_NOTIFY_STATE" \
        "$NOW_EPOCH" \
        "$NOW_EPOCH" \
        "$NOW_EPOCH" \
        1 \
        "$NOTIFY_REASON" \
        "$SELECTED_NOTIFY_SEVERITY" \
        "$SELECTED_NOTIFY_ROUTE" \
        "$SELECTED_NOTIFY_POLICY_NAME" \
        "$SELECTED_NOTIFY_SIGNAL_KEY" \
        "$SELECTED_NOTIFY_POLICY_OCCURRENCES" \
        "$SELECTED_NOTIFY_POLICY_ACTIVE_SECONDS"
    fi
  else
    rm -f "$NOTIFY_STATE_PATH"
  fi
fi

if [[ "$OUTPUT_MODE" == "compact" ]]; then
  jq -c \
    --arg notify_reason "$NOTIFY_REASON" \
    --arg notify_severity "$SELECTED_NOTIFY_SEVERITY" \
    --arg notify_route "$SELECTED_NOTIFY_ROUTE" \
    --arg notify_policy_name "$SELECTED_NOTIFY_POLICY_NAME" \
    --arg notify_policy_summary "$POLICY_SUMMARY_TEXT" \
    --arg notify_policy_summary_ultra_short "$POLICY_SUMMARY_ULTRA_SHORT_TEXT" \
    --arg notify_policy_summary_full "$POLICY_SUMMARY_FULL_TEXT" \
    --argjson notify_policy_escalated "$SELECTED_NOTIFY_POLICY_ESCALATED" \
    --argjson notify_policy_group_escalated "$SELECTED_NOTIFY_POLICY_GROUP_ESCALATED" \
    --arg notify_signal_key "$SELECTED_NOTIFY_SIGNAL_KEY" \
    --arg notify_signal_keys "$SELECTED_NOTIFY_SIGNAL_KEYS" \
    --arg notify_previous_severity "$SELECTED_NOTIFY_PREVIOUS_SEVERITY" \
    --arg notify_previous_signal_key "$SELECTED_NOTIFY_PREVIOUS_SIGNAL_KEY" \
    --argjson notify_policy_occurrences "$SELECTED_NOTIFY_POLICY_OCCURRENCES" \
    --argjson notify_policy_active_seconds "$SELECTED_NOTIFY_POLICY_ACTIVE_SECONDS" \
    --argjson sent "$notify_sent" \
    --argjson eligible "$notify_eligible" \
    --argjson policy_candidates "$POLICY_CANDIDATES_JSON" \
    --argjson policy_selection_trace "$POLICY_SELECTION_TRACE_JSON" \
    --argjson changes_only "$([[ "$NOTIFY_CHANGES_ONLY" == "1" ]] && echo true || echo false)" \
    --argjson reminder_seconds "$NOTIFY_REMINDER_SECS" \
    --argjson recovery_enabled "$([[ "$NOTIFY_RECOVERY" == "1" ]] && echo true || echo false)" \
    '. + {notify:{reason:$notify_reason,severity:$notify_severity,route:$notify_route,policy_name:$notify_policy_name,policy_summary:$notify_policy_summary,policy_summary_levels:{ultra_short:$notify_policy_summary_ultra_short,short:$notify_policy_summary,full:$notify_policy_summary_full},policy_escalated:$notify_policy_escalated,policy_group_escalated:$notify_policy_group_escalated,signal_key:$notify_signal_key,signal_keys:$notify_signal_keys,previous_severity:$notify_previous_severity,previous_signal_key:$notify_previous_signal_key,policy_occurrences:$notify_policy_occurrences,policy_active_seconds:$notify_policy_active_seconds,policy_candidates:$policy_candidates,policy_selection_trace:$policy_selection_trace,sent:$sent,eligible:$eligible,changes_only:$changes_only,recovery_enabled:$recovery_enabled,reminder_seconds:$reminder_seconds}}' \
    "$LAST_PATH"
else
  jq \
    --arg notify_reason "$NOTIFY_REASON" \
    --arg notify_severity "$SELECTED_NOTIFY_SEVERITY" \
    --arg notify_route "$SELECTED_NOTIFY_ROUTE" \
    --arg notify_policy_name "$SELECTED_NOTIFY_POLICY_NAME" \
    --arg notify_policy_summary "$POLICY_SUMMARY_TEXT" \
    --arg notify_policy_summary_ultra_short "$POLICY_SUMMARY_ULTRA_SHORT_TEXT" \
    --arg notify_policy_summary_full "$POLICY_SUMMARY_FULL_TEXT" \
    --argjson notify_policy_escalated "$SELECTED_NOTIFY_POLICY_ESCALATED" \
    --argjson notify_policy_group_escalated "$SELECTED_NOTIFY_POLICY_GROUP_ESCALATED" \
    --arg notify_signal_key "$SELECTED_NOTIFY_SIGNAL_KEY" \
    --arg notify_signal_keys "$SELECTED_NOTIFY_SIGNAL_KEYS" \
    --arg notify_previous_severity "$SELECTED_NOTIFY_PREVIOUS_SEVERITY" \
    --arg notify_previous_signal_key "$SELECTED_NOTIFY_PREVIOUS_SIGNAL_KEY" \
    --argjson notify_policy_occurrences "$SELECTED_NOTIFY_POLICY_OCCURRENCES" \
    --argjson notify_policy_active_seconds "$SELECTED_NOTIFY_POLICY_ACTIVE_SECONDS" \
    --argjson sent "$notify_sent" \
    --argjson eligible "$notify_eligible" \
    --argjson policy_candidates "$POLICY_CANDIDATES_JSON" \
    --argjson policy_selection_trace "$POLICY_SELECTION_TRACE_JSON" \
    --argjson changes_only "$([[ "$NOTIFY_CHANGES_ONLY" == "1" ]] && echo true || echo false)" \
    --argjson reminder_seconds "$NOTIFY_REMINDER_SECS" \
    --argjson recovery_enabled "$([[ "$NOTIFY_RECOVERY" == "1" ]] && echo true || echo false)" \
    '. + {notify:{reason:$notify_reason,severity:$notify_severity,route:$notify_route,policy_name:$notify_policy_name,policy_summary:$notify_policy_summary,policy_summary_levels:{ultra_short:$notify_policy_summary_ultra_short,short:$notify_policy_summary,full:$notify_policy_summary_full},policy_escalated:$notify_policy_escalated,policy_group_escalated:$notify_policy_group_escalated,signal_key:$notify_signal_key,signal_keys:$notify_signal_keys,previous_severity:$notify_previous_severity,previous_signal_key:$notify_previous_signal_key,policy_occurrences:$notify_policy_occurrences,policy_active_seconds:$notify_policy_active_seconds,policy_candidates:$policy_candidates,policy_selection_trace:$policy_selection_trace,sent:$sent,eligible:$eligible,changes_only:$changes_only,recovery_enabled:$recovery_enabled,reminder_seconds:$reminder_seconds}}' \
    "$LAST_PATH"
fi

exit "$CHECK_EXIT"
