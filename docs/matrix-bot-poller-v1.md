# Matrix Bot Poller v1 (Bot mode first slice)

## Goal

Real bot-mode ingestion by polling Matrix `/sync` and forwarding text messages to
`matrix-bot-relay`.

```text
Matrix homeserver /_matrix/client/v3/sync
  -> matrix-bot-poller
  -> matrix-bot-relay /v1/inbound/matrix-event
  -> matrix-entry-adapter
  -> consumer-entry-api
  -> CEX
```

## New app

- `apps/matrix-bot-poller`

## What it does

- calls `GET /_matrix/client/v3/sync` continuously
- keeps `next_batch` in local state file
- extracts `m.room.message` with `msgtype = m.text`
- ignores self-sent events by `MATRIX_BOT_USER_ID`
- forwards event payload to `matrix-bot-relay`

## Environment variables

- `MATRIX_POLL_HOMESERVER` (default `http://127.0.0.1:8008`)
- `MATRIX_ACCESS_TOKEN` (required to call sync, and usually also needed by relay send)
- `MATRIX_BOT_RELAY_BASE_URL` (default `http://127.0.0.1:8092`)
- `MATRIX_BOT_USER_ID` (default `@cex-bot:local.dev`)
- `MATRIX_POLL_INTERVAL_MS` (default `3000`)
- `MATRIX_SYNC_FILTER` (optional, default empty, maps to `filter` query only when set)
- `MATRIX_POLL_MAX_RECENT_EVENT_IDS` (optional, default `1000`, dedupe in-memory cache size)
- `MATRIX_SYNC_STATE_FILE` (default `/tmp/matrix-bot-poller.state`)

## Run order for local dry-run

```bash
cd /home/qian-qi/CEX
cargo run -p consumer-entry-api
cargo run -p matrix-entry-adapter
cargo run -p matrix-bot-relay
```

Linux 下一键启动链路（无 token 时会跳过 poller）：

```bash
cd /home/qian-qi/CEX
./scripts/start-matrix-bot-chain.sh
```

Then run poller (replace token):

```bash
MATRIX_ACCESS_TOKEN=<user_or_bot_token> \
cargo run -p matrix-bot-poller
```

## Notes

- This is intentionally minimal first slice for visibility.
- It now keeps a persisted recent-event dedupe window (default 1000 ids), so replayed events after `next_batch` retries won't be re-forwarded.
- For real Synapse homeservers, leave `MATRIX_SYNC_FILTER` empty unless you have created a valid filter id. Sending the old default `0` causes `/sync` to fail with `M_INVALID_PARAM: No such filter`.
- Once token + bot integration is stable, next step is appservice mode.
