#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
LOG_DIR="$ROOT_DIR/logs/matrix-bot-entry"
RUN_DIR="$ROOT_DIR/run/matrix-bot-entry"
ENV_FILE="$ROOT_DIR/.env"

if [[ -f "$ENV_FILE" ]]; then
  set -a
  while IFS='' read -r line || [[ -n "$line" ]]; do
    line="${line%$'\r'}"
    if [[ -z "$line" || "$line" == \#* ]]; then
      continue
    fi
    export "$line"
  done < "$ENV_FILE"
  set +a
fi

: "${CONSUMER_ENTRY_BIND_ADDR:=127.0.0.1:8090}"
: "${MATRIX_ENTRY_ADAPTER_BIND_ADDR:=127.0.0.1:8091}"
: "${MATRIX_BOT_RELAY_BIND_ADDR:=127.0.0.1:8092}"
: "${MATRIX_ACCESS_TOKEN:=}"
: "${MATRIX_ENTRY_INGRESS_TOKEN:=}"
: "${MATRIX_POLL_STATE_FILE:=/tmp/matrix-bot-poller.state}"
: "${MATRIX_BOT_USER_ID:=@cex-bot:localhost}"

PASS=0
FAIL=0

pass() {
  echo "[PASS] $1"
  PASS=$((PASS+1))
}

fail() {
  echo "[FAIL] $1"
  FAIL=$((FAIL+1))
}

note() {
  echo "[NOTE] $1"
}

require_health() {
  local name="$1"
  local base="$2"
  if curl -fsS "http://$base/health" > /tmp/matrix-check-http.out 2>/tmp/matrix-check-http.err; then
    pass "$name 健康检查通过"
    return 0
  else
    fail "$name 健康检查失败（请先启动服务）"
    return 1
  fi
}

extract_task_id() {
  local json="$1"

  local direct
  direct="$(printf '%s' "$json" | jq -r '(.forwarded.task_id // .forwarded.invocation_id // .forwarded.id // .task_id // .id // empty)')"

  if [[ -n "$direct" && "$direct" != "null" ]]; then
    printf '%s' "$direct"
    return
  fi

  local body
  body="$(printf '%s' "$json" | jq -r '.projected_reply.body // empty')"
  if [[ -n "$body" && "$body" != "null" ]]; then
    printf '%s' "$(printf '%s' "$body" | sed -n 's/.*Task: \([0-9a-zA-Z-]\+\).*/\1/p')"
  fi
}

log_tail_has_task() {
  local task_id="$1"
  for f in "$LOG_DIR"/*.log; do
    if [[ -f "$f" ]] && grep -q "$task_id" "$f"; then
      echo "- $f"
      return 0
    fi
  done
  return 1
}

post_matrix_event() {
  local token_mode="${1:-none}" # none, env, explicit
  local explicit_token="${2:-}"
  local status_code
  local temp_body
  local -a curl_headers=("-H" "content-type: application/json")

  if [[ "$token_mode" == "env" && -n "$MATRIX_ENTRY_INGRESS_TOKEN" ]]; then
    curl_headers+=("-H" "x-entry-token: $MATRIX_ENTRY_INGRESS_TOKEN")
  elif [[ "$token_mode" == "explicit" && -n "$explicit_token" ]]; then
    curl_headers+=("-H" "x-entry-token: $explicit_token")
  fi

  temp_body="$(mktemp)"
  set +e
  status_code="$(curl -sS -o "$temp_body" -w '%{http_code}' "${curl_headers[@]}" -X POST "http://$MATRIX_ENTRY_ADAPTER_BIND_ADDR/v1/matrix/events" -d "$payload")"
  local rc=$?
  set -e

  MATRIX_EVENT_STATUS="${status_code:-000}"
  MATRIX_EVENT_BODY=""
  if [[ $rc -eq 0 ]]; then
    MATRIX_EVENT_BODY="$(cat "$temp_body")"
  else
    MATRIX_EVENT_STATUS="000"
  fi

  rm -f "$temp_body"
}

check_projection() {
  local task_id="$1"
  local token_mode="${2:-none}" # none, env, explicit
  local explicit_token="${3:-}"
  local status_code
  local temp_body
  local -a curl_headers=()

  if [[ "$token_mode" == "env" && -n "$MATRIX_ENTRY_INGRESS_TOKEN" ]]; then
    curl_headers+=("-H" "x-entry-token: $MATRIX_ENTRY_INGRESS_TOKEN")
  elif [[ "$token_mode" == "explicit" && -n "$explicit_token" ]]; then
    curl_headers+=("-H" "x-entry-token: $explicit_token")
  fi

  temp_body="$(mktemp)"
  set +e
  status_code="$(curl -sS -o "$temp_body" -w '%{http_code}' "${curl_headers[@]}" "http://$MATRIX_ENTRY_ADAPTER_BIND_ADDR/v1/matrix/tasks/$task_id/projection")"
  local rc=$?
  set -e

  MATRIX_PROJECTION_STATUS="${status_code:-000}"
  MATRIX_PROJECTION_BODY=""
  if [[ $rc -eq 0 ]]; then
    MATRIX_PROJECTION_BODY="$(cat "$temp_body")"
  else
    MATRIX_PROJECTION_STATUS="000"
  fi

  rm -f "$temp_body"
}

wait_for_log_file() {
  if [[ -d "$RUN_DIR" ]] && compgen -G "$RUN_DIR/*.pid" > /dev/null; then
    pass "rundir 与 pid 文件存在"
  else
    note "未发现 run/pid 文件，可能不是由 start-matrix-bot-chain.sh 启动"
  fi
}

echo "== Matrix 入口链路验收清单 v1 =="
echo "root: $ROOT_DIR"

echo "1) 健康检查"
require_health "consumer-entry-api" "$CONSUMER_ENTRY_BIND_ADDR" || true
require_health "matrix-entry-adapter" "$MATRIX_ENTRY_ADAPTER_BIND_ADDR" || true
require_health "matrix-bot-relay" "$MATRIX_BOT_RELAY_BIND_ADDR" || true

echo
if [[ -n "${MATRIX_ACCESS_TOKEN}" ]]; then
  echo "2) Poller 验证"
  if [[ -f "$MATRIX_POLL_STATE_FILE" ]]; then
    pass "poller state 文件存在: $MATRIX_POLL_STATE_FILE"
  else
    note "poller state 文件不存在（首次启动前自然为空，首次运行后会创建）"
  fi
else
  note "未设置 MATRIX_ACCESS_TOKEN，跳过 poller 校验"
fi

TASK_TEXT="/task 验证移动端命令闭环测试 $(date +%s)"
EVENT_ID="event-$(date +%s)-${RANDOM}"
if [[ -n "${MATRIX_BOT_USER_ID}" ]]; then
  ROOM_ID="!e2e-room:localhost"
else
  ROOM_ID="!e2e-room:local.dev"
fi

payload=$(cat <<JSON
{
  "event_id": "${EVENT_ID}",
  "event_type": "m.room.message",
  "room_id": "${ROOM_ID}",
  "sender": "@alice:localhost",
  "text": "${TASK_TEXT}"
}
JSON
)

echo "3) 发起 /task 并检查投影"
if [[ -n "$MATRIX_ENTRY_INGRESS_TOKEN" ]]; then
  post_matrix_event none
  if [[ "$MATRIX_EVENT_STATUS" == "401" ]] && echo "$MATRIX_EVENT_BODY" | jq -e '.error == "missing or invalid entry token"' >/dev/null 2>&1; then
    pass "未带 x-entry-token 请求被拒绝（401）"
  else
    fail "未带 x-entry-token 请求未按预期被拒绝"
  fi

  post_matrix_event explicit "wrong-token"
  if [[ "$MATRIX_EVENT_STATUS" == "401" ]] && echo "$MATRIX_EVENT_BODY" | jq -e '.error == "missing or invalid entry token"' >/dev/null 2>&1; then
    pass "携带错误 x-entry-token 请求被拒绝（401）"
  else
    fail "携带错误 x-entry-token 请求未按预期被拒绝"
  fi

  post_matrix_event env
else
  post_matrix_event none
fi

response="$MATRIX_EVENT_BODY"
if [[ "$MATRIX_EVENT_STATUS" == "200" || "$MATRIX_EVENT_STATUS" == "202" ]]; then
  pass "matrix-entry-adapter 入站请求返回（status=${MATRIX_EVENT_STATUS}）"
else
  fail "matrix-entry-adapter 入站请求失败（status=${MATRIX_EVENT_STATUS}）"
  response=""
fi

if [[ -n "$response" ]]; then
  task_id="$(extract_task_id "$response")"
  if [[ -n "$task_id" ]]; then
    pass "从返回体提取到任务ID: $task_id"

    if [[ -n "$MATRIX_ENTRY_INGRESS_TOKEN" ]]; then
      check_projection "$task_id" none
      if [[ "$MATRIX_PROJECTION_STATUS" == "401" ]] && echo "$MATRIX_PROJECTION_BODY" | jq -e '.error == "missing or invalid entry token"' >/dev/null 2>&1; then
        pass "projection 未带 x-entry-token 请求被拒绝（401）"
      else
        fail "projection 未带 x-entry-token 请求未按预期被拒绝"
      fi

      check_projection "$task_id" env
      if [[ "$MATRIX_PROJECTION_STATUS" == "200" ]]; then
        if echo "$MATRIX_PROJECTION_BODY" | jq -e '.projected_reply.body' >/dev/null 2>&1; then
          pass "任务投影接口可用，返回 projected_reply"
        else
          fail "任务投影返回缺少 projected_reply"
        fi
      else
        fail "任务投影接口调用失败（status=${MATRIX_PROJECTION_STATUS}）"
      fi
    else
      if proj=$(curl -sS "http://$MATRIX_ENTRY_ADAPTER_BIND_ADDR/v1/matrix/tasks/$task_id/projection"); then
        if echo "$proj" | jq -e '.projected_reply.body' >/dev/null 2>&1; then
          pass "任务投影接口可用，返回 projected_reply"
        else
          fail "任务投影返回缺少 projected_reply"
        fi
      else
        fail "任务投影接口调用失败"
      fi
    fi
  else
    fail "未在响应里提取到任务ID"
  fi
else
  fail "未能拿到 adapter 响应"
fi

echo
if [[ -n "${task_id:-}" ]]; then
  echo "4) 关键日志命中检查"
  if log_tail_has_task "$task_id"; then
    pass "发现 task id 出现在服务日志里"
  else
    note "未在近期日志里看到 task id；建议查看 run/log 目录确认"
  fi
fi

echo
wait_for_log_file

echo
if [[ "$FAIL" -eq 0 ]]; then
  echo "结论：PASS ($PASS 项通过, $FAIL 项失败)"
  exit 0
else
  echo "结论：FAIL ($PASS 项通过, $FAIL 项失败)"
  exit 1
fi
