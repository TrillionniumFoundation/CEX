use super::*;
use trnm_world_api::{
    world_runtime_adapter_readiness, WorldActorIdentity, WorldEvidenceReceipt, WorldEvidenceSink,
    WorldIdentityAdapter, WorldLedgerAdapter, WorldLedgerReceipt, WorldMetricReceipt,
    WorldMetricsSink, WorldRepository as TrnmWorldRepository, WorldRepositoryReceipt,
    WorldRuntimeAdapterReadiness, WorldSessionDecision, WorldSessionGuard,
    WORLD_RUNTIME_ADAPTER_CONTRACT,
};
use trnm_world_domain::{
    WorldEdge as TrnmWorldEdge, WorldNode as TrnmWorldNode, WorldNpc as TrnmWorldNpc,
    WorldPosition as TrnmWorldPosition, WorldReceipt as TrnmWorldReceipt,
    WorldState as TrnmWorldState, WorldTacticsGameSession as TrnmTacticsGameSession,
    WorldTacticsRewardSettlement as TrnmTacticsRewardSettlement,
    WorldTacticsSimulationTick as TrnmTacticsSimulationTick, WorldTask as TrnmWorldTask,
    WORLD_DOMAIN_CONTRACT,
};
use trnm_world_projection::{
    WorldRouteAcceptanceRecord, WorldRouteCancellationRecord, WorldRouteCompletionRecord,
    WorldRouteContractRecord, WorldRouteDeliveryRecord, WorldRouteEventRecord,
    WorldRoutePurchaseRecord, WorldRouteRecords, WorldRouteRejectionRecord, WorldRouteReopenRecord,
    WorldRouteWorkOrderRecord,
};

const CEX_TRILLIONNIUM_WORLD_ADAPTER_CONTRACT: &str =
    "cex_trillionnium_world_production_adapter_v1";

#[derive(Clone, Copy)]
struct CexTrillionniumWorldAdapters<'a> {
    league: &'a LeagueState,
}

impl<'a> CexTrillionniumWorldAdapters<'a> {
    fn new(league: &'a LeagueState) -> Self {
        Self { league }
    }

    fn receipt_id(&self, prefix: &str, value: &str) -> String {
        league_hash_id(prefix, value)
    }

    fn state_hash(&self) -> String {
        league_state_hash(self.league).unwrap_or_else(|err| format!("state-hash-error:{err}"))
    }
}

impl WorldIdentityAdapter for CexTrillionniumWorldAdapters<'_> {
    fn resolve_actor(&self, actor_id: &str) -> WorldActorIdentity {
        if let Some(player) = self.league.players_by_matrix_user.get(actor_id) {
            return WorldActorIdentity {
                adapter_contract: CEX_TRILLIONNIUM_WORLD_ADAPTER_CONTRACT.to_string(),
                actor_id: player.player_id.clone(),
                matrix_user_id: player.matrix_user_id.clone(),
                display_name: player.display_name.clone(),
                source_of_truth: "cex_league_state.players_by_matrix_user".to_string(),
            };
        }

        WorldActorIdentity {
            adapter_contract: CEX_TRILLIONNIUM_WORLD_ADAPTER_CONTRACT.to_string(),
            actor_id: actor_id.to_string(),
            matrix_user_id: actor_id.to_string(),
            display_name: actor_id.to_string(),
            source_of_truth: "cex_world_adapter_fallback_actor_id".to_string(),
        }
    }
}

impl WorldSessionGuard for CexTrillionniumWorldAdapters<'_> {
    fn authorize_world_session(&self, actor_id: &str) -> WorldSessionDecision {
        let known_actor = self.league.players_by_matrix_user.contains_key(actor_id)
            || self
                .league
                .world
                .world_player_positions
                .contains_key(actor_id);
        WorldSessionDecision {
            adapter_contract: CEX_TRILLIONNIUM_WORLD_ADAPTER_CONTRACT.to_string(),
            accepted: known_actor,
            session_id: format!("cex-world-session:{actor_id}"),
            actor_id: actor_id.to_string(),
            reason: if known_actor {
                "cex_actor_present_in_player_or_world_position_store".to_string()
            } else {
                "cex_actor_missing_from_player_and_world_position_store".to_string()
            },
            source_of_truth: "cex_existing_web_and_api_session_guards".to_string(),
        }
    }
}

impl WorldLedgerAdapter for CexTrillionniumWorldAdapters<'_> {
    fn reserve_reward(&self, route_task_id: &str, amount_units: u64) -> WorldLedgerReceipt {
        WorldLedgerReceipt {
            adapter_contract: CEX_TRILLIONNIUM_WORLD_ADAPTER_CONTRACT.to_string(),
            receipt_id: self.receipt_id(
                "cex-trillionnium-ledger-reserve",
                &format!("{route_task_id}:{amount_units}:{}", self.state_hash()),
            ),
            route_task_id: route_task_id.to_string(),
            amount_units,
            status: "cex_ledger_reserve_path_mapped_existing_command_handlers".to_string(),
            source_of_truth: "cex_world_contract_and_tactics_ledger_settlement".to_string(),
        }
    }

    fn release_reward(&self, receipt_id: &str) -> WorldLedgerReceipt {
        WorldLedgerReceipt {
            adapter_contract: CEX_TRILLIONNIUM_WORLD_ADAPTER_CONTRACT.to_string(),
            receipt_id: receipt_id.to_string(),
            route_task_id: "cex-existing-route-task".to_string(),
            amount_units: 0,
            status: "cex_ledger_release_path_mapped_existing_command_handlers".to_string(),
            source_of_truth: "cex_world_contract_and_tactics_ledger_settlement".to_string(),
        }
    }
}

impl TrnmWorldRepository for CexTrillionniumWorldAdapters<'_> {
    fn load_world(&self, actor_id: &str) -> TrnmWorldState {
        cex_world_to_trnm_world(self.league, actor_id)
    }

    fn load_route_records(&self, _actor_id: &str, _world: &TrnmWorldState) -> WorldRouteRecords {
        cex_world_route_records(self.league)
    }

    fn save_world(
        &self,
        world: &TrnmWorldState,
        records: &WorldRouteRecords,
    ) -> WorldRepositoryReceipt {
        WorldRepositoryReceipt {
            adapter_contract: CEX_TRILLIONNIUM_WORLD_ADAPTER_CONTRACT.to_string(),
            receipt_id: self.receipt_id(
                "cex-trillionnium-repository-save",
                &format!(
                    "{}:{}:{}",
                    world.contract_version,
                    route_record_count(records),
                    self.state_hash()
                ),
            ),
            state_contract: world.contract_version.clone(),
            route_record_count: route_record_count(records),
            status: "cex_repository_bridge_ready_persistence_stays_in_existing_async_command_path"
                .to_string(),
            source_of_truth: "cex_league_repository_normalized_world_tables".to_string(),
        }
    }
}

impl WorldEvidenceSink for CexTrillionniumWorldAdapters<'_> {
    fn record_evidence(&self, evidence_kind: &str) -> WorldEvidenceReceipt {
        WorldEvidenceReceipt {
            adapter_contract: CEX_TRILLIONNIUM_WORLD_ADAPTER_CONTRACT.to_string(),
            receipt_id: self.receipt_id(
                "cex-trillionnium-evidence",
                &format!("{evidence_kind}:{}", self.state_hash()),
            ),
            evidence_kind: evidence_kind.to_string(),
            status: "cex_world_events_and_route_records_available_as_evidence".to_string(),
            source_of_truth: "cex_world_events_contracts_commerce_tactics_records".to_string(),
        }
    }
}

impl WorldMetricsSink for CexTrillionniumWorldAdapters<'_> {
    fn record_metric(&self, metric_name: &str, value: i64) -> WorldMetricReceipt {
        WorldMetricReceipt {
            adapter_contract: CEX_TRILLIONNIUM_WORLD_ADAPTER_CONTRACT.to_string(),
            receipt_id: self.receipt_id(
                "cex-trillionnium-metric",
                &format!("{metric_name}:{value}:{}", self.state_hash()),
            ),
            metric_name: metric_name.to_string(),
            value,
            status: "cex_world_adapter_metric_projected_for_readiness".to_string(),
            source_of_truth: "cex_consumer_entry_metrics_projection".to_string(),
        }
    }
}

pub(super) async fn get_trillionnium_world_adapter_readiness(
    State(state): State<AppState>,
) -> Json<Value> {
    let league = state.inner.league_state.lock().await;
    Json(cex_trillionnium_world_adapter_readiness_json_for_league(
        &league,
    ))
}

pub(super) fn cex_trillionnium_world_adapter_readiness_json_for_league(
    league: &LeagueState,
) -> Value {
    let adapters = CexTrillionniumWorldAdapters::new(league);
    let actor_id = default_adapter_actor_id(league);
    let identity = adapters.resolve_actor(&actor_id);
    let session = adapters.authorize_world_session(&actor_id);
    let world = adapters.load_world(&actor_id);
    let route_records = adapters.load_route_records(&actor_id, &world);
    let route_record_count = route_record_count(&route_records);
    let ledger_reserve = adapters.reserve_reward("cex-trillionnium-route-adapter-readiness", 1);
    let ledger_release = adapters.release_reward(&ledger_reserve.receipt_id);
    let repository = adapters.save_world(&world, &route_records);
    let evidence = adapters.record_evidence("cex_trillionnium_world_adapter_readiness");
    let metric = adapters.record_metric(
        "cex_trillionnium_world_route_record_count",
        route_record_count as i64,
    );
    let standalone_readiness = world_runtime_adapter_readiness();

    json!({
        "contract_version": CEX_TRILLIONNIUM_WORLD_ADAPTER_CONTRACT,
        "protocol_contract": WORLD_RUNTIME_ADAPTER_CONTRACT,
        "domain_contract": WORLD_DOMAIN_CONTRACT,
        "status": "cex_production_adapter_bridge_ready",
        "actor_id": actor_id,
        "source_of_truth": "cex_consumer_entry_trillionnium_world_adapters",
        "standalone_runtime_adapter_readiness": production_readiness_statuses(standalone_readiness),
        "identity": identity,
        "session": session,
        "repository": repository,
        "ledger": {
            "reserve": ledger_reserve,
            "release": ledger_release,
        },
        "evidence": evidence,
        "metric": metric,
        "cex_state_counts": cex_world_state_counts(league, route_record_count),
        "standalone_world_counts": {
            "nodes": world.nodes.len(),
            "edges": world.edges.len(),
            "positions": world.positions.len(),
            "npcs": world.npcs.len(),
            "tasks": world.tasks.len(),
            "receipts": world.receipts.len(),
        },
        "route_records": {
            "events": route_records.events.len(),
            "contracts": route_records.contracts.len(),
            "completions": route_records.completions.len(),
            "purchases": route_records.purchases.len(),
            "work_orders": route_records.work_orders.len(),
            "deliveries": route_records.deliveries.len(),
            "acceptances": route_records.acceptances.len(),
            "rejections": route_records.rejections.len(),
            "reopens": route_records.reopens.len(),
            "cancellations": route_records.cancellations.len(),
            "tactics_sessions": route_records.tactics_sessions.len(),
            "tactics_ticks": route_records.tactics_ticks.len(),
            "reward_settlements": route_records.reward_settlements.len(),
            "total": route_record_count,
        },
    })
}

fn production_readiness_statuses(
    readiness: WorldRuntimeAdapterReadiness,
) -> WorldRuntimeAdapterReadiness {
    WorldRuntimeAdapterReadiness {
        cutover_status: "cex_production_impls_connected_to_standalone_traits".to_string(),
        cex_dependency_status:
            "consumer_entry_api_depends_on_trnm_world_api_without_trnm_world_importing_cex"
                .to_string(),
        statuses: readiness
            .statuses
            .into_iter()
            .map(|mut status| {
                status.status = "cex_production_impl_connected".to_string();
                status.fixture_adapter_available = true;
                status.production_adapter_trait_ready = true;
                status.source_of_truth =
                    "cex_consumer_entry_trillionnium_world_adapters".to_string();
                status
            })
            .collect(),
        ..readiness
    }
}

fn default_adapter_actor_id(league: &LeagueState) -> String {
    league
        .world
        .world_player_positions
        .keys()
        .next()
        .cloned()
        .or_else(|| league.players_by_matrix_user.keys().next().cloned())
        .unwrap_or_else(|| "adapter-readiness".to_string())
}

fn cex_world_to_trnm_world(league: &LeagueState, actor_id: &str) -> TrnmWorldState {
    let mut nodes: Vec<TrnmWorldNode> = league
        .world
        .world_map_nodes
        .values()
        .map(|node| {
            let mut tags = node.interaction_tags.clone();
            tags.extend(node.freedom_hooks.iter().cloned());
            tags.sort();
            tags.dedup();
            TrnmWorldNode {
                id: node.node_id.clone(),
                name: node.name.clone(),
                region: node.zone_id.clone(),
                location_id: node.location_id.clone(),
                node_kind: node.node_kind.clone(),
                description: node.description.clone(),
                status: node.status.clone(),
                lat_e7: clamp_i64_to_i32(node.y),
                lng_e7: clamp_i64_to_i32(node.x),
                tags,
            }
        })
        .collect();
    nodes.sort_by(|left, right| left.id.cmp(&right.id));

    let mut edges: Vec<TrnmWorldEdge> = league
        .world
        .world_map_nodes
        .values()
        .flat_map(|node| {
            node.exits
                .iter()
                .map(move |(direction, target_node_id)| TrnmWorldEdge {
                    from: node.node_id.clone(),
                    to: target_node_id.clone(),
                    direction: direction.clone(),
                })
        })
        .collect();
    edges.sort_by(|left, right| {
        left.from
            .cmp(&right.from)
            .then(left.direction.cmp(&right.direction))
            .then(left.to.cmp(&right.to))
    });

    let mut positions: Vec<TrnmWorldPosition> = league
        .world
        .world_player_positions
        .values()
        .map(|position| TrnmWorldPosition {
            actor_id: position.matrix_user_id.clone(),
            node_id: position.node_id.clone(),
            source_of_truth: "cex_world_player_positions".to_string(),
        })
        .collect();
    if positions.is_empty() {
        positions.push(TrnmWorldPosition {
            actor_id: actor_id.to_string(),
            node_id: first_node_id(&league.world),
            source_of_truth: "cex_adapter_default_position".to_string(),
        });
    }
    positions.sort_by(|left, right| left.actor_id.cmp(&right.actor_id));

    let mut npcs: Vec<TrnmWorldNpc> = league
        .world
        .world_entities
        .values()
        .filter(|entity| {
            entity.entity_kind.contains("npc")
                || entity.role.contains("mentor")
                || entity.role.contains("npc")
        })
        .map(|entity| TrnmWorldNpc {
            id: entity.entity_id.clone(),
            name: entity.name.clone(),
            node_id: node_for_location(&league.world, &entity.location_id),
            teaches_skills: Vec::new(),
        })
        .collect();
    npcs.sort_by(|left, right| left.id.cmp(&right.id));

    let mut tasks: Vec<TrnmWorldTask> = league
        .world
        .world_contracts
        .iter()
        .map(|contract| TrnmWorldTask {
            id: contract.task_id.clone(),
            title: contract.title.clone(),
            node_id: node_for_location(&league.world, &contract.location_id),
            reward_units: contract.value_score.max(0) as u64,
            ledger_settlement_required: true,
        })
        .collect();
    tasks.sort_by(|left, right| left.id.cmp(&right.id));

    let mut receipts = Vec::new();
    receipts.extend(
        league
            .term_exchange_receipts
            .values()
            .map(term_receipt_to_world_receipt),
    );
    receipts.extend(
        league
            .world
            .world_term_exchange_receipts
            .values()
            .map(term_receipt_to_world_receipt),
    );
    receipts.sort_by(|left, right| left.id.cmp(&right.id));
    receipts.dedup_by(|left, right| left.id == right.id);

    TrnmWorldState {
        contract_version: WORLD_DOMAIN_CONTRACT.to_string(),
        source: "cex_consumer_entry_world_state_adapter".to_string(),
        nodes,
        edges,
        positions,
        npcs,
        tasks,
        receipts,
    }
}

fn cex_world_route_records(league: &LeagueState) -> WorldRouteRecords {
    WorldRouteRecords {
        events: league
            .world
            .world_events
            .iter()
            .map(|event| WorldRouteEventRecord {
                event_id: event.event_id.clone(),
                event_kind: event.event_kind.clone(),
                location_id: event.location_id.clone(),
                task_id: event.cex_task_id.clone().unwrap_or_default(),
                route_status: event
                    .cex_status
                    .clone()
                    .unwrap_or_else(|| event.result.clone()),
                body: event.body.clone(),
                impact_score: event.impact_score,
                created_at_epoch: event.created_at_epoch,
            })
            .collect(),
        contracts: league
            .world
            .world_contracts
            .iter()
            .map(|contract| WorldRouteContractRecord {
                contract_id: contract.contract_id.clone(),
                task_id: contract.task_id.clone(),
                title: contract.title.clone(),
                body: contract.body.clone(),
                location_id: contract.location_id.clone(),
                status: contract.status.clone(),
                value_score: contract.value_score,
                created_at_epoch: contract.created_at_epoch,
            })
            .collect(),
        completions: league
            .world
            .world_contract_completions
            .iter()
            .map(|completion| {
                let contract = league
                    .world
                    .world_contracts
                    .iter()
                    .find(|contract| contract.contract_id == completion.contract_id);
                WorldRouteCompletionRecord {
                    completion_id: completion.completion_id.clone(),
                    contract_id: completion.contract_id.clone(),
                    task_id: contract
                        .map(|contract| contract.task_id.clone())
                        .unwrap_or_default(),
                    location_id: contract
                        .map(|contract| contract.location_id.clone())
                        .unwrap_or_default(),
                    body: completion.body.clone(),
                    ledger_status: completion.ledger_status.clone(),
                    payout_status: completion.payout_status.clone(),
                    score: completion.score,
                    reward_amount: completion.reward_amount,
                    created_at_epoch: completion.created_at_epoch,
                }
            })
            .collect(),
        purchases: league
            .world
            .world_purchases
            .iter()
            .map(|purchase| WorldRoutePurchaseRecord {
                purchase_id: purchase.purchase_id.clone(),
                listing_id: purchase.listing_id.clone(),
                company_id: purchase.company_id.clone(),
                location_id: location_for_company(&league.world, &purchase.company_id),
                status: purchase.status.clone(),
                price_credits: purchase.price_credits,
                created_at_epoch: purchase.created_at_epoch,
            })
            .collect(),
        work_orders: league
            .world
            .world_work_orders
            .iter()
            .map(|work_order| WorldRouteWorkOrderRecord {
                work_order_id: work_order.work_order_id.clone(),
                listing_id: work_order.listing_id.clone(),
                purchase_id: work_order.purchase_id.clone(),
                location_id: location_for_company(&league.world, &work_order.company_id),
                brief: work_order.brief.clone(),
                status: work_order.status.clone(),
                value_score: work_order.value_score,
                created_at_epoch: work_order.created_at_epoch,
            })
            .collect(),
        deliveries: league
            .world
            .world_work_deliveries
            .iter()
            .map(|delivery| WorldRouteDeliveryRecord {
                delivery_id: delivery.delivery_id.clone(),
                work_order_id: delivery.work_order_id.clone(),
                location_id: location_for_work_order(&league.world, &delivery.work_order_id),
                body: delivery.body.clone(),
                status: delivery.status.clone(),
                score: delivery.score,
                created_at_epoch: delivery.created_at_epoch,
            })
            .collect(),
        acceptances: league
            .world
            .world_work_acceptances
            .iter()
            .map(|acceptance| WorldRouteAcceptanceRecord {
                acceptance_id: acceptance.acceptance_id.clone(),
                work_order_id: acceptance.work_order_id.clone(),
                location_id: location_for_work_order(&league.world, &acceptance.work_order_id),
                body: acceptance.body.clone(),
                status: acceptance.status.clone(),
                reputation_delta: acceptance.reputation_delta,
                created_at_epoch: acceptance.created_at_epoch,
            })
            .collect(),
        rejections: league
            .world
            .world_work_rejections
            .iter()
            .map(|rejection| WorldRouteRejectionRecord {
                rejection_id: rejection.rejection_id.clone(),
                work_order_id: rejection.work_order_id.clone(),
                location_id: location_for_work_order(&league.world, &rejection.work_order_id),
                body: rejection.body.clone(),
                status: rejection.status.clone(),
                refund_status: rejection.refund_status.clone(),
                created_at_epoch: rejection.created_at_epoch,
            })
            .collect(),
        reopens: league
            .world
            .world_work_reopens
            .iter()
            .map(|reopen| WorldRouteReopenRecord {
                reopen_id: reopen.reopen_id.clone(),
                work_order_id: reopen.work_order_id.clone(),
                location_id: location_for_work_order(&league.world, &reopen.work_order_id),
                body: reopen.body.clone(),
                status: reopen.status.clone(),
                reserve_status: reopen.reserve_status.clone(),
                created_at_epoch: reopen.created_at_epoch,
            })
            .collect(),
        cancellations: league
            .world
            .world_work_cancellations
            .iter()
            .map(|cancellation| WorldRouteCancellationRecord {
                cancellation_id: cancellation.cancellation_id.clone(),
                work_order_id: cancellation.work_order_id.clone(),
                location_id: location_for_work_order(&league.world, &cancellation.work_order_id),
                body: cancellation.body.clone(),
                status: cancellation.status.clone(),
                refund_status: cancellation.refund_status.clone(),
                created_at_epoch: cancellation.created_at_epoch,
            })
            .collect(),
        tactics_sessions: cex_tactics_sessions_to_trnm(league),
        tactics_ticks: cex_tactics_ticks_to_trnm(league),
        reward_settlements: cex_reward_settlements_to_trnm(league),
    }
}

fn cex_tactics_sessions_to_trnm(league: &LeagueState) -> Vec<TrnmTacticsGameSession> {
    let mut sessions: Vec<TrnmTacticsGameSession> = league
        .world
        .world_tactics_sessions
        .values()
        .map(|session| TrnmTacticsGameSession {
            session_id: session.session_id.clone(),
            matrix_user_id: session.matrix_user_id.clone(),
            room_id: session.room_id.clone(),
            active_node_id: session.active_node_id.clone(),
            active_overlay_id: session.active_overlay_id.clone(),
            objective_id: session.objective_id.clone(),
            objective_progress: session.objective_progress,
            objective_goal: session.objective_goal,
            victory_state: session.victory_state.clone(),
            reward_status: session.reward_status.clone(),
            reward_event_id: session.reward_event_id.clone(),
            reward_credits_awarded: session.reward_credits_awarded,
            reward_xp_awarded: session.reward_xp_awarded,
            created_at_epoch: session.created_at_epoch,
            updated_at_epoch: session.updated_at_epoch,
        })
        .collect();
    sessions.sort_by(|left, right| left.session_id.cmp(&right.session_id));
    sessions
}

fn cex_tactics_ticks_to_trnm(league: &LeagueState) -> Vec<TrnmTacticsSimulationTick> {
    let mut ticks: Vec<TrnmTacticsSimulationTick> = league
        .world
        .world_tactics_simulation_ticks
        .iter()
        .map(|tick| TrnmTacticsSimulationTick {
            tick_id: tick.tick_id.clone(),
            session_id: tick.session_id.clone(),
            matrix_user_id: tick.matrix_user_id.clone(),
            command: tick.command.clone(),
            outcome_accepted: tick.outcome_accepted,
            outcome_result: tick.outcome_result.clone(),
            action_cost: (tick.action_points_before - tick.action_points_after).max(0),
            effect_summary: tick.simulation_effect.clone(),
            created_at_epoch: tick.created_at_epoch,
        })
        .collect();
    ticks.sort_by(|left, right| left.tick_id.cmp(&right.tick_id));
    ticks
}

fn cex_reward_settlements_to_trnm(league: &LeagueState) -> Vec<TrnmTacticsRewardSettlement> {
    let mut settlements: Vec<TrnmTacticsRewardSettlement> = league
        .world
        .world_tactics_sessions
        .values()
        .filter(|session| session.reward_status != "not_eligible")
        .map(|session| TrnmTacticsRewardSettlement {
            settlement_id: league_hash_id(
                "cex-trillionnium-tactics-reward-settlement",
                &session.session_id,
            ),
            session_id: session.session_id.clone(),
            route_task_id: format!(
                "tactics-objective:{}:{}",
                session.matrix_user_id, session.objective_id
            ),
            ledger_receipt_id: session.reward_event_id.clone(),
            reward_status: session.reward_status.clone(),
            credits_delta: session.reward_credits_awarded,
            xp_delta: session.reward_xp_awarded,
            source_of_truth: "cex_world_tactics_sessions".to_string(),
        })
        .collect();
    settlements.sort_by(|left, right| left.settlement_id.cmp(&right.settlement_id));
    settlements
}

fn route_record_count(records: &WorldRouteRecords) -> usize {
    records.events.len()
        + records.contracts.len()
        + records.completions.len()
        + records.purchases.len()
        + records.work_orders.len()
        + records.deliveries.len()
        + records.acceptances.len()
        + records.rejections.len()
        + records.reopens.len()
        + records.cancellations.len()
        + records.tactics_sessions.len()
        + records.tactics_ticks.len()
        + records.reward_settlements.len()
}

fn cex_world_state_counts(league: &LeagueState, route_record_count: usize) -> Value {
    json!({
        "players": league.players_by_matrix_user.len(),
        "world_zones": league.world.world_zones.len(),
        "world_locations": league.world.world_locations.len(),
        "world_entities": league.world.world_entities.len(),
        "world_map_nodes": league.world.world_map_nodes.len(),
        "world_player_positions": league.world.world_player_positions.len(),
        "world_contracts": league.world.world_contracts.len(),
        "world_contract_completions": league.world.world_contract_completions.len(),
        "world_purchases": league.world.world_purchases.len(),
        "world_work_orders": league.world.world_work_orders.len(),
        "world_tactics_sessions": league.world.world_tactics_sessions.len(),
        "world_tactics_ticks": league.world.world_tactics_simulation_ticks.len(),
        "term_exchange_receipts": league.term_exchange_receipts.len(),
        "world_term_exchange_receipts": league.world.world_term_exchange_receipts.len(),
        "route_records": route_record_count,
    })
}

fn term_receipt_to_world_receipt(receipt: &TermExchangeReceiptState) -> TrnmWorldReceipt {
    TrnmWorldReceipt {
        id: receipt.receipt_id.clone(),
        progression_class: format!("{:?}", receipt.progression_class),
        status: format!("{:?}", receipt.status),
    }
}

fn first_node_id(world: &WorldState) -> String {
    world
        .world_map_nodes
        .keys()
        .next()
        .cloned()
        .unwrap_or_else(|| default_world_node_id().to_string())
}

fn node_for_location(world: &WorldState, location_id: &str) -> String {
    world
        .world_map_nodes
        .values()
        .find(|node| node.location_id == location_id)
        .map(|node| node.node_id.clone())
        .unwrap_or_else(|| first_node_id(world))
}

fn location_for_company(world: &WorldState, company_id: &str) -> String {
    world
        .world_companies
        .iter()
        .find(|company| company.company_id == company_id)
        .map(|company| company.location_id.clone())
        .unwrap_or_default()
}

fn location_for_work_order(world: &WorldState, work_order_id: &str) -> String {
    world
        .world_work_orders
        .iter()
        .find(|work_order| work_order.work_order_id == work_order_id)
        .map(|work_order| location_for_company(world, &work_order.company_id))
        .unwrap_or_default()
}

fn clamp_i64_to_i32(value: i64) -> i32 {
    value.clamp(i32::MIN as i64, i32::MAX as i64) as i32
}
