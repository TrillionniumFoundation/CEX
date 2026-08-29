# Matrix 入口链路验收清单 v1

> 与真实 Synapse 一致时，保持“先启动脚本、再健康检查、再命令闭环、再回看日志”的可复用流程。

## 一键快速复现

### 真实 Matrix/Element 房间闭环（推荐本地验收）

```bash
cd /data/home-data/CEX
CEX_ENV_FILE=run/local-production/.env ./scripts/start-matrix-live-stack.sh
./scripts/check-matrix-live-room-e2e.sh
```

`start-matrix-live-stack.sh` 会用 Docker 启动本地 Synapse 与 Element Web，创建/复用 `@alice:local.dev`、`@cex-bot:local.dev`、`CEX Frontend E2E` 房间，登录 bot、启动 `matrix-bot-relay` 与 `matrix-bot-poller`，并把 Element Web 暴露在 `http://127.0.0.1:8081`。脚本会把 Matrix token 写入 `run/matrix-live/tokens.json` / `live-env.sh`，这些文件权限为 `0600`，不要提交或打印 token。

`check-matrix-live-room-e2e.sh` 会从真实房间发送并等待 bot 回房间：

1. `/task ...` -> CEX 任务卡
2. `/status <task-id>` -> 同任务执行状态卡
3. `/balance` -> 钱包卡片
4. `/plans` -> 套餐卡片

验收摘要保存到 `run/matrix-live/e2e-summary-<epoch>.json`，只记录房间 ID、事件 ID、任务 ID 与回复内容，不记录 token。

### 无真实 homeserver 的链路冒烟

```bash
cd /data/home-data/CEX
./scripts/start-matrix-bot-chain.sh
./scripts/check-matrix-bot-entry-v1.sh
```

> 说明：`start-matrix-bot-chain.sh` 会启动 `consumer-entry-api`、`matrix-entry-adapter`、`matrix-bot-relay`，若检测到 `MATRIX_ACCESS_TOKEN` 再加 `matrix-bot-poller`。
>
> 建议本地验收时先执行 `start-matrix-bot-chain.sh`，再执行检查脚本。

## 核心验收项目

1. **健康检查**
   - `GET http://127.0.0.1:8090/health`
   - `GET http://127.0.0.1:8091/health`
   - `GET http://127.0.0.1:8092/health`
   - 如果有 poller：确认 `MATRIX_POLL_STATE_FILE` 已创建/有更新。

2. **端到端任务闭环（纯脚本）**
   - 通过 `POST /v1/matrix/events` 提交文本：`/task ...`
   - 如果配置 `MATRIX_ENTRY_INGRESS_TOKEN`，会先做鉴权回归：
     - 无 token 应返回 401 且报错 `missing or invalid entry token`
     - 错 token 应返回 401 且报错同上
     - 正确 token 应继续正常进入闭环
   - 解析 adapter 返回中的 `forwarded.task_id`
   - 访问 `GET /v1/matrix/tasks/<task-id>/projection`
     - 同样会校验 `x-entry-token`，缺 token 应 401，正确 token 应返回 `projected_reply`
   - 确认返回含 `projected_reply`

3. **状态回写可见性**
   - 查询刚创建任务的 projection 时，应见到 `consumer_status` 与 `invocation_status`
   - 检查 relay/adapter 日志是否包含 task id（用于排错）

4. **命令行为回归（移动端 v1）**
   - `/help` 命中帮助投影，不创建任务
   - `/status <id>` 返回该任务投影
   - `/task cap=... account=... 文本` 透传可选参数
   - `/balance` / `/wallet` / `/余额` / `/钱包` 返回钱包卡片
   - `/plans` / `/plan` / `/package` / `/套餐` 返回套餐卡片
   - 未知命令返回 `/help` 提示

5. **真实 Matrix/Element 前端闭环**
   - Synapse `GET /_matrix/client/versions` 正常
   - Element Web `GET /config.json` 指向同一 homeserver
   - `matrix-bot-relay /health` 中 `has_access_token=true`
   - 真实房间内用户发送 `/task` 后，bot 回同房间 `🧾 CEX 任务卡`
   - 同房间继续发送 `/status`、`/balance`、`/plans`，均可收到带 `formatted_body` 与 `cex_card` 的投影回复
   - Trillionnium League / World 命令也应闭环：`/app`（必须 module_count >= 5，含 map/duel/social/wallet/progression，且 `map_engine_id=leaflet_openstreetmap_v1` / `tile_provider=OpenStreetMap` / `map_renderer_adapter_id=leaflet_renderer_adapter_v1` / `map_runtime_handle_name=mapRuntime` / `map_renderer_future_engine_candidate=maplibre_gl_v1` / `progression_level>=1`，并暴露 `route_next_opportunity_action_label` / `route_next_opportunity_panel_id` / `route_next_opportunity_textarea_id` / `route_next_opportunity_body` 这组可直接驱动移动端打开下一机会 lane 的目标字段）、`/social`（必须 contact_count >= 1）、`/duel nearby ...`（必须 match_id=face-duel-001 且有 task_id）、`/league`、`/world`、`/map`（必须 node_count >= 8，且 `has_real_world_map_engine=true` / `map_engine_id=leaflet_openstreetmap_v1` / `mirror_scope=global_real_world_tiles` / renderer adapter contract 与 `/app` 一致）、`/go ...`（必须返回 to_node_id/location_id）、`/world action ...`、`/craft ...`、`/assets`、`/upgrade latest ...`（必须 value_delta >= 1）、`/companies`、`/company latest ...`（必须返回 company_id/revenue_score）、`/shops`、`/sell latest ...`（必须返回 listing_id/price_credits/quality_score）、`/buy latest ...`（必须返回 purchase_id/work_order_id 且 ledger_status=settled）、`/work deliver latest ...`（必须返回 delivery_id/status=delivered）、`/work accept latest ...`（必须返回 acceptance_id/status=accepted）、`/work reject latest ...`（必须返回 rejection_id/status=rejected_refunded 且 buyer_refund_status=refunded）、`/work reopen latest ...`（必须返回 reopen_id/status=reopened 且 buyer_reopen_reserve_status=reserved）、返工 `/work deliver latest ...` + `/work accept latest ...`（必须 buyer_consume_status=consumed）、取消流 `/buy latest ...` + `/work cancel latest ...`（必须 status=cancelled_refunded 且 buyer_cancel_refund_status=refunded）、`/work`（必须 work_order_count >= 1 且 delivery_count/acceptance_count/rejection_count/reopen_count/cancellation_count >= 1），并且 `/assets` / `/upgrade latest ...` / `/companies` / `/company latest ...` / `/shops` / `/sell latest ...` / `/buy latest ...` / `/work deliver latest ...` / `/work accept latest ...` / `/work` / `/work reject latest ...` / `/work reopen latest ...` / `/work cancel latest ...` 这些 route-adjacent 卡片都必须暴露 `route_next_opportunity_action_label` / `route_next_opportunity_panel_id` / `route_next_opportunity_input_id` / `route_next_opportunity_input_value` / `route_next_opportunity_textarea_id` / `route_next_opportunity_target_node_id`，证明移动端能直接把下一机会命令映射到对应 lane、表单和 node focus；`/factions`（必须 faction_count >= 4 且 standing_count >= 1）、`/contract ...`（必须返回 CEX `task_id` 和 `contract_id`）、`/complete <contract-id> ...`（必须 `ledger_status=settled` 且 judge 含 hidden）、`/season`、`/arena`、`/guild`、`/guild guild-prompt-forge`、`/raid`、`/team guild-raid-001 scout`、`/raid guild-raid-001 ...`、`/draft ...`、`/join daily-dungeon-001`、`/battle ...`、`/submit ...`、提交卡 `ledger_status=settled`、再次 `/balance` 余额变化、`/profile`、`/progression`（level >= 1 且 successful_task_count >= 1）、`/skills`、`/tools`、`/skins`（catalog 存在且至少解锁一个）、`/rewards`、`/inventory`、`/history`、`/rank`、`/loadout`
   - Web E2E 还应覆盖 `GET /world`、`POST /world/web/action`，并确认 World Action Console、Asset Upgrade form、Contract completion form、资产/事件时间线与 world marker 都能回显。
   - `/submit ...` 的 `league_submission` 卡片应包含 `ledger_status=settled`，且 `judge_status` 走 `rubric_hidden_*`、`score_event_count >= 7`，证明本地 reward 已走 ledger grant 结算且 Judge Pipeline v2 已启用。
   - review/admin API 应能列出 `held_review` 奖励，并通过 approve/reject 把 `review_status` 写回；approve 成功时应触发同一条 ledger grant 释放路径。
   - Web 游戏壳应通过 `scripts/check-trillionnium-league-web-e2e.sh` 验证：`GET /league`、`/league/web/session` 签发 HttpOnly web session、带 cookie+CSRF 的 action、join/guild/team/raid/draft/submit 表单、Battle Timeline、settled reward、以及 anti-cheat `held_review` 展示。
   - SQL cutover bridge 应通过 `scripts/check-trillionnium-league-sql-snapshot.sh` 验证：生成 `league-state-snapshot.sql`，包含 `league_state_snapshots` insert、`sha256:` state hash，并且 `/v1/league/state/snapshot` 返回 object counts。

## 关键日志文件（默认）

- `/data/home-data/CEX/logs/matrix-bot-entry/consumer-entry-api.log`
- `/data/home-data/CEX/logs/matrix-bot-entry/matrix-entry-adapter.log`
- `/data/home-data/CEX/logs/matrix-bot-entry/matrix-bot-relay.log`
- `/data/home-data/CEX/logs/matrix-bot-entry/matrix-bot-poller.log`（有 token 启动时）

## 现网凭证注意（真实 Synapse）

- 真实 Synapse 通常要求 `MATRIX_SYNC_FILTER` 保持空，不要配置为 `0`
- `MATRIX_POLL_HOMESERVER` 与 `MATRIX_HOMESERVER_BASE_URL` 含义要分离
- 先将 `.env` 写到固定路径（canonical）再启动，避免脚本参数污染

## 命令参考

```bash
curl -sS http://127.0.0.1:8091/v1/matrix/tasks/<task-id>/projection | jq
./scripts/check-matrix-bot-entry-v1.sh
./scripts/check-matrix-live-room-e2e.sh
```
