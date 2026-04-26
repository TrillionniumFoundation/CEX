# Trillionnium League / World Game Design v1

> Product direction: turn CEX from a chat task entry into a playable AI-agent esports platform. Users do not just submit jobs; they enter leagues, build agent lineups, clear quests, fight ranked matches, raid large tasks, and earn rewards through verified skill and useful work.

## 1. North Star

**Trillionnium League** is a skill-to-earn AI Agent esports league. **Trillionnium World** is the larger high-freedom reality mirror around it: a sandbox where players build companies, shops, Agent teams, guilds, assets, relationships, and real-task economies.

- Dota inspiration: draft, roles, lanes/objectives, team fights, ranked ladder, spectators.
- World of Warcraft inspiration: classes, guilds, quests, raids, dungeons, loot, reputation, seasons.
- CEX-native twist: every battle is backed by a real useful task, benchmark, customer bounty, or platform challenge.

One-line pitch:

> Enter Trillionnium World. Build your Agent life, company, guild, and assets. Enter the League when you want competition. Clear real-world quests. Win reputation, credits, and bounty rewards.

## 2. Product Pillars

1. **Playable first**
   - The user should feel they are entering a match, not filling a form.
   - Every task has tension: time, opponents, scoring, rewards, progress, reveal.

2. **Skill over gambling**
   - No user-vs-user betting as the core loop.
   - Rewards come from verified output quality, customer acceptance, benchmark score, contribution, or sponsored prize pools.

3. **Human + Agent mastery**
   - The player is not replaced by AI; the player pilots, drafts, tunes, and coordinates agents.
   - Good players win by strategy, prompt craft, tool choice, timing, review discipline, and team coordination.

4. **Persistent identity**
   - Players accumulate rank, class mastery, agent loadouts, item/skill unlocks, match history, and reputation.

5. **Real economy, game shell**
   - Credits power compute and entry fees.
   - Earned balance tracks verified rewards.
   - Cosmetic/status rewards increase retention without distorting real-money fairness.

## 3. Core Game Loop

```text
Lobby -> Scout quest/match -> Draft agent lineup -> Execute rounds -> Review/counterplay -> Submit -> Judge -> Reward -> Rank up -> Upgrade loadout
```

### 3.1 Moment-to-moment loop

1. Player opens League lobby.
2. Chooses a mode: Solo Queue, Bounty Arena, Guild Raid, Daily Dungeon, Ranked Ladder.
3. Sees objective, reward pool, timer, recommended roles, scoring rules.
4. Drafts a squad: agent heroes + tools + model budget + prompt deck.
5. Starts execution. CEX shows live status as a battle feed.
6. Player can intervene between phases: refine, rerun, attach evidence, challenge judge, request teammate review.
7. Final submission is scored.
8. Rewards settle to wallet and leaderboard.

## 4. Game Modes

### 4.1 Solo Queue

Fast 3-10 minute matches for one user.

- Good for onboarding.
- Example: summarize a messy document, create a landing page copy, solve a coding kata, clean a dataset.
- Score: quality x speed x cost efficiency.

### 4.2 Bounty Arena

Real task with prize pool.

- Sponsor/customer posts task and bounty.
- Multiple players or teams compete.
- Top placements split bounty.
- Customer acceptance can override or weight final score.

### 4.3 Guild Raid

Large task split into phases, WoW-style.

- Roles: Scout, Builder, Reviewer, Finisher, Strategist.
- Raid phases: Discovery -> Plan -> Execute -> QA -> Delivery.
- Each phase has boss mechanics: missing data, ambiguity, low-quality drafts, deadline pressure.
- Rewards split by contribution score.

### 4.4 Ranked Ladder

Daily/weekly benchmark matches.

- Same task seed for all players.
- Anti-cheat: hidden tests, shuffled variants, result audit, judge-disagreement flags, and payout review holds.
- Ranks: Bronze -> Silver -> Gold -> Platinum -> Diamond -> Mythic -> Trillionnaire.

### 4.5 Daily Dungeon

Repeatable PvE challenge.

- Short task with known scoring rubric.
- Gives XP, credits, small rewards, loot boxes/items.
- Helps users learn the system without risking big credits.

### 4.6 Tournament / Season Finals

Scheduled event.

- Brackets or Swiss rounds.
- Live spectator room.
- Highlights and replays.
- Sponsor prize pool.

## 5. Player Roles / Classes

Classes should be identity and UI affordances, not hard locks.

1. **Strategist**
   - Strength: planning, decomposing tasks, choosing agent lineup.
   - Bonus: better preflight plans, lower wasted budget.

2. **Scout**
   - Strength: research, sourcing, fact checking.
   - Bonus: evidence quality, source coverage.

3. **Builder**
   - Strength: creating drafts/code/assets.
   - Bonus: output completeness and speed.

4. **Auditor**
   - Strength: QA, validation, risk detection.
   - Bonus: fewer failed submissions, higher trust score.

5. **Closer**
   - Strength: packaging final delivery for client/judge.
   - Bonus: customer acceptance and presentation score.

6. **Summoner**
   - Strength: agent orchestration and prompt deck design.
   - Bonus: unlocks advanced multi-agent combos.

## 6. Agent Heroes

Agents are the player's heroes/champions.

Example hero archetypes:

- **Oracle Scout**: web/source research and brief extraction.
- **Forge Builder**: code/content generation.
- **Mirror Auditor**: critique and test generation.
- **Ledger Warden**: cost/risk optimizer.
- **Muse Designer**: visual/design direction.
- **Courier Closer**: final package + customer-facing delivery.

Hero fields:

- `hero_id`
- `class_tags`
- `skill_slots`
- `tool_affinities`
- `model_affinities`
- `energy_cost`
- `cooldowns`
- `rank_requirement`
- `cosmetic_skin_id`

## 7. Dota-like Match Mechanics

### 7.1 Draft phase

- Player/team chooses 3-5 agent heroes.
- Each match recommends required roles.
- Future PvP: ban/pick for tournament mode.

### 7.2 Lanes / objectives

For work tasks, lanes map to parallel objectives:

- **Intel Lane**: research/context.
- **Build Lane**: output creation.
- **QA Lane**: validation/tests.
- **Delivery Lane**: final packaging.

Teams win by clearing objectives efficiently, not just one final generation.

### 7.3 Cooldowns and resources

- Compute budget = mana.
- Attempts = lives.
- Tool calls = abilities.
- Human review = ultimate ability / interrupt.
- Deadline = match timer.

### 7.4 Counterplay

Counterplay should be ethical and task-centric:

- Challenge score with evidence.
- Submit improved validation.
- Reveal hallucination or bug in another solution.
- Provide stronger customer-fit reasoning.

No sabotage, spam, or prompt injection attacks as allowed gameplay.

## 8. WoW-like MMO Mechanics

### 8.1 World map

The top-level product is **Trillionnium World**. League is the arena module; Craft is the building/company/workshop module. The world should feel closer to Minecraft + The Sims + GTA-style freedom + MUD text agency, but mapped to legitimate real-world creation, work, commerce, and reputation.

The World can be divided into zones:

- **The Prompt Forge**: prompt and agent tuning quests.
- **Research Wilds**: sourcing/fact-checking challenges.
- **Code Citadel**: coding tasks and benchmark dungeons.
- **Design Atelier**: brand/image/layout quests.
- **Market Bazaar**: real customer bounties.
- **Audit Sanctum**: QA, compliance, risk review.

Current first slice exposes this as `GET /world` plus Matrix `/world`, `/world action <自由行动>`, `/assets`, `/upgrade <asset-id|latest> <升级内容>`, `/companies`, `/company <asset-id|latest> <公司方案>`, `/shops`, `/sell <company-id|latest> <服务/商品>`, `/buy <listing-id|latest> <需求>`, `/work`, `/factions`, `/contract <委托内容>`, `/complete <contract-id> <交付内容>`, and `/craft <建造内容>`. The web shell shows zones, locations, Agent residents/NPCs, player assets, upgrade form, companies/shops/listings, Commerce / Work Orders, Faction Reputation Map, World Contracts, completion form, and the world event timeline.

### 8.2 Quests

Quest types:

- Training quest: learn a command/flow.
- Daily quest: small reward.
- Bounty quest: real payout.
- Open-world action: free text intent such as “open an AI design company”, “build a shop”, “hire an Agent”, “go to the market”, or “mirror this real customer demand into the world”.
- World Company: an operating entity launched from a player asset; it has level, revenue score, reputation score, and an owner relationship.
- World Shop / Listing: a storefront and priced service offer created from a company, forming the first persistent commerce loop.
- World Purchase / Work Order: buying or hiring a listing opens a work order, credits seller revenue through Trillionnium Ledger when configured, and increases Market/Craft/City faction standing.
- World Faction Standing: reputation with City Clerks, Craft Union, Market Guild, and League Order grows from commerce/work activity and should later unlock map privileges, fees, and quests.
- World Contract: a real customer/market commission that creates a CEX invocation and becomes a trackable world object.
- Asset upgrade: judged operational/craft improvement that increases asset value/level and creates a visible growth tree.
- Contract completion: a judged delivery that can settle ledger rewards, upgrade player assets, and change reputation.
- Raid quest: team objective with shared boss progress, contribution scoring, and roster roles such as Scout, Builder, Auditor, and Closer.
- Class quest: unlock role perks.

### 8.3 Guilds

Guild features:

- Shared reputation.
- Team matchmaking.
- Guild treasury.
- Contribution ledger.
- Raid scheduling.
- Raid roster slots and role claims for Scout/Builder/Auditor/Closer-style cooperation.
- Internal coach/review roles.

### 8.4 Loot

Loot should mostly be non-pay-to-win or productivity unlocks:

- Prompt cards.
- Agent skins.
- Badge frames.
- Replay highlights.
- Queue priority earned by reputation.
- Tool unlocks based on trust/compliance.

## 9. Economy

### 9.1 Balances

- **Credits**: compute/entry fuel.
- **Earned Balance**: verified reward balance.
- **Reputation**: non-transferable trust/rank.
- **Season XP**: progression.

### 9.2 Reward sources

- Customer bounty.
- Platform weekly prize pool.
- Sponsor challenge.
- Guild raid payout.
- Prompt/agent template marketplace royalties.

### 9.3 Safety boundary

Avoid gambling mechanics:

- Do not let users wager against each other as the primary game.
- Entry fees should pay compute/platform cost or fund skill prize pools with clear rules.
- Rewards must be tied to objective scoring, customer acceptance, or contribution.
- Provide anti-fraud review and payout holds for suspicious wins.

## 10. Scoring

Score formula v1:

```text
final_score = quality * 0.45 + speed * 0.15 + cost_efficiency * 0.15 + evidence * 0.10 + customer_fit * 0.10 + sportsmanship * 0.05
```

Scoring sources:

- Automated tests for objective tasks.
- LLM judge ensemble for subjective tasks, connected through a provider-agnostic judge adapter with deterministic rubric/hidden-test fallback.
- Human/customer acceptance for bounty tasks.
- Peer challenge/audit evidence.
- Cost and latency metrics from CEX execution trace.

## 11. UI Direction

### 11.1 Matrix/Element MVP

Commands:

- `/league` - League home card.
- `/arena` - active matches.
- `/quest` - daily quests.
- `/join <match_id>` - join match.
- `/draft <hero...>` - set agent lineup.
- `/battle <match_id> <prompt>` - execute round.
- `/submit <match_id>` - submit final output.
- `/rank` - leaderboard.
- `/guild` - guild status.
- `/wallet` - credits + earned balance.

Cards:

- League home card.
- Match card.
- Battle feed card.
- Scoreboard card.
- Reward card.
- Player profile card.

### 11.2 Web game shell

Main screens:

1. League Lobby
2. World Map
3. Match Room
4. Draft Screen
5. Battle Timeline
6. Scoreboard
7. Player Profile
8. Guild Hall
9. Wallet / Rewards
10. Agent Loadout

Visual style:

- Dark esports UI.
- Hero cards for agents.
- Live battle log/timeline.
- Rank badges.
- World map zones.
- Reward chest animation after scoring.

## 12. MVP Cut

The first playable slice should not wait for full web UI.

### MVP 1: Text Arena in Matrix

- `/league` home card.
- `/arena` list of 3 static/dynamic starter matches.
- `/join` creates a player-match entry.
- `/battle` creates a CEX task tied to match.
- `/rank` shows local leaderboard.
- `/wallet` reuses existing wallet projection.

### MVP 2: Backend League Domain

Add tables:

- `league_players`
- `league_seasons`
- `league_matches`
- `league_match_entries`
- `league_submissions`
- `league_score_events`
- `league_reward_events`
- `league_player_ratings`
- `league_agent_loadouts`
- `league_guilds`

### MVP 3: Web Shell

Build first real game UI after Matrix commands prove the loop.

## 13. Integration With Existing CEX

Existing CEX primitive -> League concept:

- `Invocation` -> Battle action / skill cast.
- `Execution` -> Combat log / live action.
- `Account` -> credit wallet.
- `LedgerEntry` -> reward/cost settlement.
- `Capability` -> hero skill / tool ability.
- `Policy` -> anti-cheat / safety rules.
- `AuditEvent` -> replay log / match integrity.
- Matrix command cards -> first League frontend.

## 14. Immediate Build Order

1. Add docs and naming: Trillionnium League.
2. Add `/league` and `/arena` Matrix cards.
3. Add minimal league read model in consumer-entry-api.
4. Add match join and battle linkage to existing task creation.
5. Add scoreboard projection.
6. Add reward settlement through ledger grant entries, with idempotent League reward IDs and Matrix-visible settlement status.
7. Build web shell once command loop feels playable.
8. Upgrade web shell into a local-dev playable Battle Console with forms for join/guild/draft/submit, live replay timeline, guild halls, and loadout display.
9. Next: replace deterministic scoring with judge events (objective tests + LLM/human review) and season/guild ladders backed by SQL.
