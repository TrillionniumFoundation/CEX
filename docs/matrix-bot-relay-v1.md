# Matrix Bot Relay v1

## Goal

Close the first end-to-end loop:

```text
Matrix room event
  -> matrix-bot-relay
  -> matrix-entry-adapter
  -> consumer-entry-api
  -> CEX gateway
  -> projected reply
  -> Matrix room send API
```

This gives the repo its first runnable skeleton for:

- ingesting a Matrix-shaped event
- creating a CEX task
- projecting consumer status
- sending a reply back into the Matrix room

## New app

- `apps/matrix-bot-relay`

## Endpoints

### Health

`GET /health`

### Observability fields

`GET /health` now returns an `observability` block with queue and send telemetry. Useful fields include:

- `inbound_events_total`: total inbound matrix events received
- `self_events_total`: filtered self-sent events
- `duplicate_events_total`: duplicated `event_id` events filtered
- `projected_reply_missing_total`: adapter responses without `projected_reply`
- `adapter_requests_total` / `adapter_failures_total`
- `matrix_send_attempts_total` / `matrix_send_successes_total` / `matrix_send_failures_total`
- `direct_send_attempts_total` / `direct_send_successes_total` / `direct_send_failures_total`
- `queue_send_attempts_total` / `queue_send_success_total` / `queue_send_failures_total`
- `queue_send_requeues_total` / `queue_send_drops_total`
- `queue_len_current` / `queue_len_peak` / `queue_len_avg`
- `enqueued_total`

See the runtime values in `/health` during smoke tests to confirm retry queue behavior.

### Inbound Matrix event

`POST /v1/inbound/matrix-event`

Example payload:

```json
{
  "event_id": "$event-123",
  "event_type": "m.room.message",
  "room_id": "!roomid:local.dev",
  "sender": "@alice:local.dev",
  "text": "Summarize this document"
}
```

## Behavior

The relay:

1. ignores self-sent events from the configured bot user
2. drops duplicate `event_id` within in-memory window
3. forwards the event to `matrix-entry-adapter /v1/matrix/events`
4. reads the returned `projected_reply`
5. if a Matrix access token is configured, sends that reply to the room using:
   - `PUT /_matrix/client/v3/rooms/{roomId}/send/m.room.message/{txnId}`
6. retry logic:
   - immediate inline retries (configurable)
   - durable persistent queue on retryable failures
7. `/health` exposes queue length/status
8. API errors return normalized shape (`error_code`, `error`, `details`)
9. if no access token is configured yet, it still returns the projected reply so the chain can be tested without live Matrix sending

## Environment variables

- `MATRIX_BOT_RELAY_BIND_ADDR` (default `127.0.0.1:8092`)
- `MATRIX_ADAPTER_BASE_URL` (default `http://127.0.0.1:8091`)
- `MATRIX_ENTRY_INGRESS_TOKEN` (optional, forwarded as `x-entry-token` to matrix-entry-adapter)
- `MATRIX_HOMESERVER_BASE_URL` (default `http://127.0.0.1:8008`)
- `MATRIX_ACCESS_TOKEN` (required for actual room sends)
- `MATRIX_BOT_USER_ID` (default `@cex-bot:local.dev`)
- `MATRIX_RELAY_MAX_RECENT_EVENT_IDS` (optional, default `1000`)
- `MATRIX_RELAY_SEND_MAX_ATTEMPTS` (optional, default `3`)
- `MATRIX_RELAY_SEND_INITIAL_DELAY_MS` (optional, default `200`)
- `MATRIX_RELAY_SEND_MAX_DELAY_MS` (optional, default `2500`)
- `MATRIX_RELAY_QUEUE_ENABLED` (optional, default `true`)
- `MATRIX_RELAY_QUEUE_PATH` (optional, default `/tmp/matrix-bot-relay-queue.json`)
- `MATRIX_RELAY_QUEUE_POLL_INTERVAL_MS` (optional, default `4000`)
- `MATRIX_RELAY_QUEUE_MAX_SIZE` (optional, default `2000`)

## Run locally

```bash
cd /home/qian-qi/CEX
cargo run -p consumer-entry-api
cargo run -p matrix-entry-adapter
cargo run -p matrix-bot-relay
```

### Linux one-command start for matrix chain

```bash
cd /home/qian-qi/CEX
./scripts/start-matrix-bot-chain.sh [smoke]
```

- 默认会启动 `consumer-entry-api:8090`、`matrix-entry-adapter:8091`、`matrix-bot-relay:8092`
- 若设置 `MATRIX_ACCESS_TOKEN`，再启动 `matrix-bot-poller`
- 添加 `smoke` 会额外跑一次 `/health` 与 `POST /v1/inbound/matrix-event` 冒烟

## Local smoke without live Matrix send

Leave `MATRIX_ACCESS_TOKEN` empty and call:

```bash
curl -s http://127.0.0.1:8092/health | jq

curl -s -X POST http://127.0.0.1:8092/v1/inbound/matrix-event \
  -H 'content-type: application/json' \
  -d '{
    "event_id":"$event-123",
    "event_type":"m.room.message",
    "room_id":"!roomid:local.dev",
    "sender":"@alice:local.dev",
    "text":"Summarize this document"
  }' | jq
```

This should still show:

- upstream task creation response
- the projected Matrix reply payload
- `sent_to_matrix: false`

## Local smoke with live Matrix send

Set:

- `MATRIX_HOMESERVER_BASE_URL`
- `MATRIX_ACCESS_TOKEN`
- `MATRIX_BOT_USER_ID`

Then call the same inbound endpoint. The relay will try to send the projected reply to the room.

If Matrix send errors are retryable, they are queued in `MATRIX_RELAY_QUEUE_PATH` and retried in the background.

## What this is NOT yet

Still missing:

- appservice registration and namespace routing
- richer message formatting / cards / buttons
- confirmation callbacks back into CEX approvals
- retry jitter + congestion control under burst traffic

## Recommended next step

After this, use bot mode with the poller for a full loop:

- `matrix-bot-poller` handles Matrix `/sync`
- forwards events into `matrix-bot-relay`

For current stage, this is still the fastest path to a visible demo.
