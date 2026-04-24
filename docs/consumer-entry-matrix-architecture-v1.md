# Matrix / Element as Telegram-like C-end Entry for CEX v1

## 1. Goal

Use a **Telegram-like chat UI** as the C-end entry layer for CEX, while keeping CEX as the backend execution / billing / audit core.

The target is **not** to expose raw CEX service semantics directly to consumers.
Instead, add a product-facing entry layer that translates chat interactions into CEX invocations.

## 2. Recommended high-level stack

```text
Consumer App Shell
  ├─ Element-based Web / Mobile shell
  ├─ Custom chat UI, task cards, wallet/credits pages
  └─ Push notifications

Matrix Interaction Layer
  ├─ Matrix homeserver (Synapse or compatible)
  ├─ Rooms / DMs / events / media / presence
  └─ Bot or bridge user for product workflows

Product Entry Layer (new)
  ├─ consumer-entry-api / BFF
  ├─ session + conversation model
  ├─ user profile / entitlement / package model
  ├─ payment callback integration
  ├─ chat-command / task translation
  └─ state projection for C-end UI

CEX Core
  ├─ gateway-service
  ├─ identity-service
  ├─ ledger-service
  ├─ capability-service
  ├─ execution-service
  └─ audit-service
```

## 3. Why Matrix / Element is the best fit here

Compared with Rocket.Chat / Mattermost / Signal-like options, Matrix + Element is the most suitable when the goal is:

- a Telegram-like interaction shell
- open source and self-hosted
- event-driven integration
- rooms / DMs / notifications / media already solved
- flexible enough to add product cards, AI task updates, and bot-style flows

In this setup:

- **Element** is the visible product shell
- **Matrix** is the messaging/event transport
- **CEX** is the execution / credit / audit engine

## 4. Core design principle

### Do not let C-end talk directly in CEX-native language

CEX currently speaks in backend-native concepts such as:

- invocation
- execution
- reserve / consume / refund
- approval checkpoint
- audit trace

C-end should instead see product-native language such as:

- message
- task
- generation
- balance / credits
- package / subscription
- confirm / retry
- completed / failed / refunded

So the new product entry layer must translate between the two.

## 5. Recommended service split

### 5.1 Matrix side

Owns:
- user-facing chat rooms / DMs
- message transport
- push notifications
- media and attachments
- bot account presence

### 5.2 Product entry layer (new)

Owns:
- mapping Matrix user -> product user -> CEX org/actor/account
- conversation/session state
- front-end task model
- prompt shaping / command parsing
- package entitlement checks
- wallet and credits presentation
- CEX invocation creation and polling
- translating execution state into chat updates and task cards

### 5.3 CEX side

Owns:
- trusted identity resolution
- credits / reserve / consume / refund
- capability registry
- execution lifecycle
- approvals and audit trail

## 6. Recommended identity mapping

Do not use Matrix identity as the final authority by itself.

Use a mapping table in the product entry layer:

```text
matrix_user_id
  -> product_user_id
  -> cex_org_id
  -> cex_actor_id
  -> cex_account_id
  -> subscription/package state
```

Then the product entry layer calls CEX using a product-controlled API key or delegated key strategy.

For C-end flows, the product entry layer should be the one that decides:

- which org/account to charge
- which capability_id to invoke
- whether the user has entitlement
- whether this action needs confirmation before dispatch

## 7. Recommended request flow

### 7.1 Basic chat-to-task path

```text
1. User sends a message in Element
2. Matrix event arrives
3. Product entry bot/webhook receives event
4. Product entry layer parses intent
5. Product entry layer builds a product task
6. Product entry layer calls CEX POST /v1/invocations
7. CEX creates invocation / reserve / execution
8. Product entry layer polls or subscribes for status
9. Product entry layer sends status back to Matrix room
10. User sees Telegram-like progress in chat UI
```

### 7.2 State translation example

Backend-facing:
- Created
- Queued
- AwaitingApproval
- Running
- Succeeded
- Failed
- Refunded

C-end-facing:
- received
- queued
- waiting for confirmation
- processing
- done
- failed
- refunded

## 8. Recommended MVP slice

Do not try to clone full Telegram at once.

Start with a narrow MVP:

### MVP v1
- single-user DM flow
- one bot identity
- one Element-based web shell
- one or two CEX capabilities
- simple credits page
- simple task history
- one confirmation flow

### MVP v1 supported actions
- send text request
- create one invocation
- show queue / running / done / failed
- show remaining credits
- confirm a gated action
- retry a failed task

This is enough to prove the architecture.

## 9. Frontend pages beyond chat

Even if chat is the primary entry, you still need a few product pages outside the message thread:

- credits / wallet page
- package / subscription page
- task history page
- profile / settings page
- capability picker or assistant mode switcher

These should live in the Element-derived shell or adjacent product web views.

## 10. CEX API usage guidance

For C-end entry, prefer this pattern:

- `POST /v1/invocations` for user-triggered tasks
- `GET /v1/invocations/:id` for task status projection
- direct admin endpoints stay behind the product entry layer only

Do **not** expose raw ledger/admin/audit APIs directly to the consumer client.

Admin token surfaces described in `docs/admin-token-model.md` should remain internal/operator-only.

## 11. Approval model recommendation

CEX already has approval-capable execution semantics.
For C-end, surface them as product confirmations:

Examples:
- "This action will consume 50 credits, continue?"
- "This tool can access an external system, confirm first"
- "This run exceeds your default budget cap"

In UI terms, approval should feel like a chat-native confirm card, not an operator console action.

## 12. Suggested first implementation path

### Phase 1
- stand up Matrix + Element locally
- create one product bot user
- create one webhook/bridge from Matrix events to a small `consumer-entry-api`
- wire one chat command to one CEX capability invocation

### Phase 2
- add task state cards
- add credits page
- add retry / confirmation interactions
- add attachment passing

### Phase 3
- multi-room / multi-assistant support
- package/subscription model
- richer workflow routing
- group / channel style entry points

## 13. Practical product framing

This should be framed internally as:

> A Telegram-like AI operating surface backed by the CEX runtime core.

Not as:

> CEX directly exposed to consumers.

That framing keeps product semantics, trust boundaries, and backend ownership cleaner.

## 14. Final recommendation

For the stated goal, the best next architecture move is:

1. keep CEX as the backend core
2. add a new `consumer-entry-api` / BFF
3. use Matrix + Element as the Telegram-like shell
4. translate chat actions into CEX invocations
5. translate CEX execution states back into consumer task states
