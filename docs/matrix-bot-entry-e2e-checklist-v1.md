# Matrix 入口链路验收清单 v1

> 与真实 Synapse 一致时，保持“先启动脚本、再健康检查、再命令闭环、再回看日志”的可复用流程。

## 一键快速复现

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
   - 未知命令返回 `/help` 提示

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
```
