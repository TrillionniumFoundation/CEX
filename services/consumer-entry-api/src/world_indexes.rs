use super::*;

#[derive(Debug, Clone, Default)]
pub(super) struct WorldIndexes {
    pub(super) sorted_zone_ids: Vec<String>,
    pub(super) sorted_location_ids: Vec<String>,
    pub(super) sorted_entity_ids: Vec<String>,
    pub(super) sorted_faction_ids: Vec<String>,
    pub(super) sorted_player_position_user_ids: Vec<String>,
    pub(super) sorted_trillionnium_character_user_ids: Vec<String>,
    pub(super) sorted_tactics_session_ids: Vec<String>,
    pub(super) sorted_map_node_ids: Vec<String>,
    pub(super) sorted_map_node_ids_by_id: Vec<String>,
    pub(super) sorted_asset_indices_by_id: Vec<usize>,
    pub(super) sorted_event_indices_by_id: Vec<usize>,
    pub(super) sorted_relationship_indices_by_id: Vec<usize>,
    pub(super) sorted_contract_indices_by_id: Vec<usize>,
    pub(super) sorted_contract_completion_indices_by_id: Vec<usize>,
    pub(super) sorted_asset_upgrade_indices_by_id: Vec<usize>,
    pub(super) sorted_company_indices_by_id: Vec<usize>,
    pub(super) sorted_shop_indices_by_id: Vec<usize>,
    pub(super) sorted_listing_indices_by_id: Vec<usize>,
    pub(super) sorted_economy_event_indices_by_id: Vec<usize>,
    pub(super) sorted_purchase_indices_by_id: Vec<usize>,
    pub(super) sorted_work_order_indices_by_id: Vec<usize>,
    pub(super) sorted_faction_standing_indices_by_id: Vec<usize>,
    pub(super) sorted_work_delivery_indices_by_id: Vec<usize>,
    pub(super) sorted_work_acceptance_indices_by_id: Vec<usize>,
    pub(super) sorted_work_rejection_indices_by_id: Vec<usize>,
    pub(super) sorted_work_reopen_indices_by_id: Vec<usize>,
    pub(super) sorted_work_cancellation_indices_by_id: Vec<usize>,
    pub(super) sorted_tactics_simulation_tick_indices_by_id: Vec<usize>,
    pub(super) map_node_ids_by_location: HashMap<String, Vec<String>>,
    pub(super) asset_index_by_id: HashMap<String, usize>,
    pub(super) contract_index_by_id: HashMap<String, usize>,
    pub(super) latest_contract_index_by_actor: HashMap<String, usize>,
    pub(super) latest_completable_contract_index_by_actor: HashMap<String, usize>,
    pub(super) contract_completion_index_by_id: HashMap<String, usize>,
    pub(super) latest_asset_index_by_owner: HashMap<String, usize>,
    pub(super) company_index_by_id: HashMap<String, usize>,
    pub(super) latest_company_index_by_owner: HashMap<String, usize>,
    pub(super) latest_operating_company_index_by_owner: HashMap<String, usize>,
    pub(super) company_location_by_id: HashMap<String, String>,
    pub(super) shop_index_by_company_id: HashMap<String, usize>,
    pub(super) shop_index_by_id: HashMap<String, usize>,
    pub(super) shop_location_by_id: HashMap<String, String>,
    pub(super) listing_index_by_id: HashMap<String, usize>,
    pub(super) latest_listed_listing_index: Option<usize>,
    pub(super) purchase_index_by_id: HashMap<String, usize>,
    pub(super) work_order_index_by_id: HashMap<String, usize>,
    pub(super) work_order_location_by_id: HashMap<String, String>,
    pub(super) event_indices_by_location: HashMap<String, Vec<usize>>,
    pub(super) latest_deliverable_work_order_by_seller: HashMap<String, usize>,
    pub(super) latest_acceptable_work_order_by_buyer: HashMap<String, usize>,
    pub(super) latest_rejectable_work_order_by_buyer: HashMap<String, usize>,
    pub(super) latest_reopenable_work_order_by_buyer: HashMap<String, usize>,
    pub(super) latest_cancellable_work_order_by_buyer: HashMap<String, usize>,
    pub(super) acceptance_index_by_id: HashMap<String, usize>,
    pub(super) rejection_index_by_id: HashMap<String, usize>,
    pub(super) reopen_index_by_id: HashMap<String, usize>,
    pub(super) cancellation_index_by_id: HashMap<String, usize>,
    pub(super) faction_standing_index_by_user_faction: HashMap<(String, String), usize>,
    pub(super) recent_asset_indices: Vec<usize>,
    pub(super) recent_company_indices: Vec<usize>,
    pub(super) recent_shop_indices: Vec<usize>,
    pub(super) recent_listing_indices: Vec<usize>,
    pub(super) recent_event_indices: Vec<usize>,
    pub(super) recent_contract_indices: Vec<usize>,
    pub(super) recent_contract_completion_indices: Vec<usize>,
    pub(super) recent_purchase_indices: Vec<usize>,
    pub(super) recent_work_order_indices: Vec<usize>,
    pub(super) recent_work_delivery_indices: Vec<usize>,
    pub(super) recent_work_acceptance_indices: Vec<usize>,
    pub(super) recent_work_rejection_indices: Vec<usize>,
    pub(super) recent_work_reopen_indices: Vec<usize>,
    pub(super) recent_work_cancellation_indices: Vec<usize>,
    pub(super) recent_faction_standing_indices: Vec<usize>,
}

pub(super) fn recent_tail_indices(len: usize, limit: usize) -> Vec<usize> {
    (0..len).rev().take(limit).collect()
}

fn world_contract_completable_for_latest(contract: &WorldContract) -> bool {
    !matches!(
        contract.status.as_str(),
        "completed_settled" | "review_hold"
    ) && !matches!(
        contract.cex_status.as_deref(),
        Some("completed" | "completed_no_reward" | "review_hold")
    )
}

pub(super) fn sorted_indices_by<T, F>(items: &[T], mut compare: F) -> Vec<usize>
where
    F: FnMut(&T, &T) -> std::cmp::Ordering,
{
    let mut indices: Vec<usize> = (0..items.len()).collect();
    indices.sort_by(|left, right| compare(&items[*left], &items[*right]));
    indices
}

pub(super) fn indexed_sorted<'a, T>(items: &'a [T], indices: &'a [usize]) -> Vec<&'a T> {
    indices
        .iter()
        .filter_map(move |index| items.get(*index))
        .collect()
}

pub(super) fn indexed_recent<'a, T>(
    items: &'a [T],
    indices: &'a [usize],
    limit: usize,
) -> impl Iterator<Item = &'a T> + 'a {
    indices
        .iter()
        .take(limit)
        .filter_map(move |index| items.get(*index))
}

#[allow(clippy::field_reassign_with_default)]
pub(super) fn build_world_indexes(world: &WorldState) -> WorldIndexes {
    let mut indexes = WorldIndexes::default();

    indexes.sorted_zone_ids = world.world_zones.keys().cloned().collect();
    indexes.sorted_zone_ids.sort();
    indexes.sorted_location_ids = world.world_locations.keys().cloned().collect();
    indexes.sorted_location_ids.sort();
    indexes.sorted_entity_ids = world.world_entities.keys().cloned().collect();
    indexes.sorted_entity_ids.sort();
    indexes.sorted_faction_ids = world.world_factions.keys().cloned().collect();
    indexes.sorted_faction_ids.sort();
    indexes.sorted_player_position_user_ids =
        world.world_player_positions.keys().cloned().collect();
    indexes.sorted_player_position_user_ids.sort();
    indexes.sorted_trillionnium_character_user_ids = world
        .world_trillionnium_characters
        .keys()
        .cloned()
        .collect();
    indexes.sorted_trillionnium_character_user_ids.sort();
    indexes.sorted_tactics_session_ids = world.world_tactics_sessions.keys().cloned().collect();
    indexes.sorted_tactics_session_ids.sort();

    indexes.sorted_map_node_ids = world.world_map_nodes.keys().cloned().collect();
    indexes.sorted_map_node_ids_by_id = indexes.sorted_map_node_ids.clone();
    indexes.sorted_map_node_ids_by_id.sort();
    indexes.sorted_map_node_ids.sort_by(|left, right| {
        match (
            world.world_map_nodes.get(left),
            world.world_map_nodes.get(right),
        ) {
            (Some(left_node), Some(right_node)) => left_node
                .y
                .cmp(&right_node.y)
                .then(left_node.x.cmp(&right_node.x))
                .then(left_node.node_id.cmp(&right_node.node_id)),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => left.cmp(right),
        }
    });

    for node in indexes
        .sorted_map_node_ids_by_id
        .iter()
        .filter_map(|node_id| world.world_map_nodes.get(node_id))
    {
        indexes
            .map_node_ids_by_location
            .entry(node.location_id.clone())
            .or_default()
            .push(node.node_id.clone());
    }
    for node_ids in indexes.map_node_ids_by_location.values_mut() {
        node_ids.sort_by(|left, right| {
            match (
                world.world_map_nodes.get(left),
                world.world_map_nodes.get(right),
            ) {
                (Some(left_node), Some(right_node)) => left_node
                    .y
                    .cmp(&right_node.y)
                    .then(left_node.x.cmp(&right_node.x))
                    .then(left_node.node_id.cmp(&right_node.node_id)),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => left.cmp(right),
            }
        });
    }

    indexes.sorted_asset_indices_by_id = sorted_indices_by(&world.world_assets, |left, right| {
        left.asset_id.cmp(&right.asset_id)
    });
    indexes.sorted_event_indices_by_id = sorted_indices_by(&world.world_events, |left, right| {
        left.event_id.cmp(&right.event_id)
    });
    indexes.sorted_relationship_indices_by_id =
        sorted_indices_by(&world.world_relationships, |left, right| {
            left.relationship_id.cmp(&right.relationship_id)
        });
    indexes.sorted_contract_indices_by_id =
        sorted_indices_by(&world.world_contracts, |left, right| {
            left.contract_id.cmp(&right.contract_id)
        });
    indexes.sorted_contract_completion_indices_by_id =
        sorted_indices_by(&world.world_contract_completions, |left, right| {
            left.completion_id.cmp(&right.completion_id)
        });
    indexes.sorted_asset_upgrade_indices_by_id =
        sorted_indices_by(&world.world_asset_upgrades, |left, right| {
            left.upgrade_id.cmp(&right.upgrade_id)
        });
    indexes.sorted_company_indices_by_id =
        sorted_indices_by(&world.world_companies, |left, right| {
            left.company_id.cmp(&right.company_id)
        });
    indexes.sorted_shop_indices_by_id = sorted_indices_by(&world.world_shops, |left, right| {
        left.shop_id.cmp(&right.shop_id)
    });
    indexes.sorted_listing_indices_by_id =
        sorted_indices_by(&world.world_listings, |left, right| {
            left.listing_id.cmp(&right.listing_id)
        });
    indexes.sorted_economy_event_indices_by_id =
        sorted_indices_by(&world.world_economy_events, |left, right| {
            left.economy_event_id.cmp(&right.economy_event_id)
        });
    indexes.sorted_purchase_indices_by_id =
        sorted_indices_by(&world.world_purchases, |left, right| {
            left.purchase_id.cmp(&right.purchase_id)
        });
    indexes.sorted_work_order_indices_by_id =
        sorted_indices_by(&world.world_work_orders, |left, right| {
            left.work_order_id.cmp(&right.work_order_id)
        });
    indexes.sorted_faction_standing_indices_by_id =
        sorted_indices_by(&world.world_faction_standings, |left, right| {
            left.standing_id.cmp(&right.standing_id)
        });
    indexes.sorted_work_delivery_indices_by_id =
        sorted_indices_by(&world.world_work_deliveries, |left, right| {
            left.delivery_id.cmp(&right.delivery_id)
        });
    indexes.sorted_work_acceptance_indices_by_id =
        sorted_indices_by(&world.world_work_acceptances, |left, right| {
            left.acceptance_id.cmp(&right.acceptance_id)
        });
    indexes.sorted_work_rejection_indices_by_id =
        sorted_indices_by(&world.world_work_rejections, |left, right| {
            left.rejection_id.cmp(&right.rejection_id)
        });
    indexes.sorted_work_reopen_indices_by_id =
        sorted_indices_by(&world.world_work_reopens, |left, right| {
            left.reopen_id.cmp(&right.reopen_id)
        });
    indexes.sorted_work_cancellation_indices_by_id =
        sorted_indices_by(&world.world_work_cancellations, |left, right| {
            left.cancellation_id.cmp(&right.cancellation_id)
        });
    indexes.sorted_tactics_simulation_tick_indices_by_id =
        sorted_indices_by(&world.world_tactics_simulation_ticks, |left, right| {
            left.tick_id.cmp(&right.tick_id)
        });

    for (index, asset) in world.world_assets.iter().enumerate() {
        indexes
            .asset_index_by_id
            .insert(asset.asset_id.clone(), index);
        indexes
            .latest_asset_index_by_owner
            .insert(asset.owner_matrix_user_id.clone(), index);
    }

    for (index, contract) in world.world_contracts.iter().enumerate() {
        indexes
            .contract_index_by_id
            .insert(contract.contract_id.clone(), index);
        indexes
            .latest_contract_index_by_actor
            .insert(contract.actor_matrix_user_id.clone(), index);
        if world_contract_completable_for_latest(contract) {
            indexes
                .latest_completable_contract_index_by_actor
                .insert(contract.actor_matrix_user_id.clone(), index);
        }
    }

    for (index, completion) in world.world_contract_completions.iter().enumerate() {
        indexes
            .contract_completion_index_by_id
            .insert(completion.completion_id.clone(), index);
    }

    for (index, company) in world.world_companies.iter().enumerate() {
        indexes
            .company_index_by_id
            .insert(company.company_id.clone(), index);
        indexes
            .latest_company_index_by_owner
            .insert(company.owner_matrix_user_id.clone(), index);
        if company.status == "operating" {
            indexes
                .latest_operating_company_index_by_owner
                .insert(company.owner_matrix_user_id.clone(), index);
        }
        indexes
            .company_location_by_id
            .insert(company.company_id.clone(), company.location_id.clone());
    }

    for (index, shop) in world.world_shops.iter().enumerate() {
        indexes
            .shop_index_by_company_id
            .entry(shop.company_id.clone())
            .or_insert(index);
        indexes.shop_index_by_id.insert(shop.shop_id.clone(), index);
        indexes
            .shop_location_by_id
            .insert(shop.shop_id.clone(), shop.location_id.clone());
    }

    for (index, listing) in world.world_listings.iter().enumerate() {
        indexes
            .listing_index_by_id
            .insert(listing.listing_id.clone(), index);
        if listing.status == "listed" {
            indexes.latest_listed_listing_index = Some(index);
        }
    }

    for (index, purchase) in world.world_purchases.iter().enumerate() {
        indexes
            .purchase_index_by_id
            .insert(purchase.purchase_id.clone(), index);
    }

    for (index, event) in world.world_events.iter().enumerate() {
        indexes
            .event_indices_by_location
            .entry(event.location_id.clone())
            .or_default()
            .push(index);
    }

    for (index, work_order) in world.world_work_orders.iter().enumerate() {
        indexes
            .work_order_index_by_id
            .insert(work_order.work_order_id.clone(), index);
        indexes.work_order_location_by_id.insert(
            work_order.work_order_id.clone(),
            indexes
                .company_location_by_id
                .get(&work_order.company_id)
                .cloned()
                .unwrap_or_default(),
        );
        if matches!(work_order.status.as_str(), "open" | "delivery_review_hold") {
            indexes
                .latest_deliverable_work_order_by_seller
                .insert(work_order.seller_matrix_user_id.clone(), index);
        }
        if work_order.status == "delivered" {
            indexes
                .latest_acceptable_work_order_by_buyer
                .insert(work_order.buyer_matrix_user_id.clone(), index);
        }
        if matches!(
            work_order.status.as_str(),
            "delivered"
                | "delivery_review_hold"
                | "rejected_refund_hold"
                | "rejected_refund_failed"
                | "rejected_chargeback_failed"
        ) {
            indexes
                .latest_rejectable_work_order_by_buyer
                .insert(work_order.buyer_matrix_user_id.clone(), index);
        }
        if work_order.status == "rejected_refunded" {
            indexes
                .latest_reopenable_work_order_by_buyer
                .insert(work_order.buyer_matrix_user_id.clone(), index);
        }
        if matches!(
            work_order.status.as_str(),
            "open"
                | "payment_hold"
                | "seller_settlement_pending"
                | "seller_settlement_failed"
                | "reopen_reserve_hold"
                | "reopen_reserve_failed"
                | "reopen_seller_settlement_pending"
                | "reopen_seller_settlement_failed"
                | "cancelled_refund_hold"
                | "cancelled_refund_failed"
                | "cancelled_chargeback_failed"
        ) {
            indexes
                .latest_cancellable_work_order_by_buyer
                .insert(work_order.buyer_matrix_user_id.clone(), index);
        }
    }

    for (index, acceptance) in world.world_work_acceptances.iter().enumerate() {
        indexes
            .acceptance_index_by_id
            .insert(acceptance.acceptance_id.clone(), index);
    }
    for (index, rejection) in world.world_work_rejections.iter().enumerate() {
        indexes
            .rejection_index_by_id
            .insert(rejection.rejection_id.clone(), index);
    }
    for (index, reopen) in world.world_work_reopens.iter().enumerate() {
        indexes
            .reopen_index_by_id
            .insert(reopen.reopen_id.clone(), index);
    }
    for (index, cancellation) in world.world_work_cancellations.iter().enumerate() {
        indexes
            .cancellation_index_by_id
            .insert(cancellation.cancellation_id.clone(), index);
    }

    for (index, standing) in world.world_faction_standings.iter().enumerate() {
        indexes.faction_standing_index_by_user_faction.insert(
            (standing.matrix_user_id.clone(), standing.faction_id.clone()),
            index,
        );
    }

    indexes.recent_event_indices = recent_tail_indices(world.world_events.len(), 12);
    indexes.recent_asset_indices = recent_tail_indices(world.world_assets.len(), 8);
    indexes.recent_company_indices = recent_tail_indices(world.world_companies.len(), 8);
    indexes.recent_shop_indices = recent_tail_indices(world.world_shops.len(), 8);
    indexes.recent_listing_indices = recent_tail_indices(world.world_listings.len(), 8);
    indexes.recent_contract_indices = recent_tail_indices(world.world_contracts.len(), 12);
    indexes.recent_contract_completion_indices =
        recent_tail_indices(world.world_contract_completions.len(), 12);
    indexes.recent_purchase_indices = recent_tail_indices(world.world_purchases.len(), 8);
    indexes.recent_work_order_indices = recent_tail_indices(world.world_work_orders.len(), 8);
    indexes.recent_work_delivery_indices =
        recent_tail_indices(world.world_work_deliveries.len(), 8);
    indexes.recent_work_acceptance_indices =
        recent_tail_indices(world.world_work_acceptances.len(), 8);
    indexes.recent_work_rejection_indices =
        recent_tail_indices(world.world_work_rejections.len(), 8);
    indexes.recent_work_reopen_indices = recent_tail_indices(world.world_work_reopens.len(), 8);
    indexes.recent_work_cancellation_indices =
        recent_tail_indices(world.world_work_cancellations.len(), 8);
    indexes.recent_faction_standing_indices =
        recent_tail_indices(world.world_faction_standings.len(), 8);

    indexes
}

impl WorldIndexes {
    pub(super) fn resolve_company_index(
        &self,
        company_id: &str,
        matrix_user_id: &str,
    ) -> Option<usize> {
        if company_id == "latest" {
            self.latest_operating_company_index_by_owner
                .get(matrix_user_id)
                .or_else(|| self.latest_company_index_by_owner.get(matrix_user_id))
                .copied()
        } else {
            self.company_index_by_id.get(company_id).copied()
        }
    }

    pub(super) fn latest_buyable_listing_index_for_buyer(
        &self,
        world: &WorldState,
        matrix_user_id: &str,
    ) -> Option<usize> {
        world
            .world_listings
            .iter()
            .enumerate()
            .rev()
            .find(|(_, listing)| {
                listing.status == "listed" && listing.owner_matrix_user_id != matrix_user_id
            })
            .map(|(index, _)| index)
            .or(self.latest_listed_listing_index)
    }

    pub(super) fn resolve_buyable_listing_index(
        &self,
        world: &WorldState,
        listing_id: &str,
        matrix_user_id: &str,
    ) -> Option<usize> {
        if listing_id == "latest" {
            self.latest_buyable_listing_index_for_buyer(world, matrix_user_id)
        } else {
            self.listing_index_by_id.get(listing_id).copied()
        }
    }

    pub(super) fn company_index(&self, company_id: &str) -> Option<usize> {
        self.company_index_by_id.get(company_id).copied()
    }

    pub(super) fn shop_index(&self, shop_id: &str) -> Option<usize> {
        self.shop_index_by_id.get(shop_id).copied()
    }

    pub(super) fn asset_index(&self, asset_id: &str) -> Option<usize> {
        self.asset_index_by_id.get(asset_id).copied()
    }

    pub(super) fn resolve_asset_index(
        &self,
        asset_id: &str,
        matrix_user_id: &str,
    ) -> Option<usize> {
        if asset_id == "latest" {
            self.latest_asset_index_for_owner(matrix_user_id)
        } else {
            self.asset_index(asset_id)
        }
    }

    pub(super) fn contract_index(&self, contract_id: &str) -> Option<usize> {
        self.contract_index_by_id.get(contract_id).copied()
    }

    pub(super) fn contract<'a>(
        &self,
        world: &'a WorldState,
        contract_id: &str,
    ) -> Option<&'a WorldContract> {
        self.contract_index(contract_id)
            .and_then(|index| world.world_contracts.get(index))
    }

    pub(super) fn company_location_id(&self, company_id: &str) -> Option<&str> {
        self.company_location_by_id
            .get(company_id)
            .map(String::as_str)
    }

    pub(super) fn shop_location_id(&self, shop_id: &str) -> Option<&str> {
        self.shop_location_by_id.get(shop_id).map(String::as_str)
    }

    pub(super) fn work_order_location_id(&self, work_order_id: &str) -> Option<&str> {
        self.work_order_location_by_id
            .get(work_order_id)
            .map(String::as_str)
    }

    pub(super) fn first_map_node_for_location<'a>(
        &self,
        world: &'a WorldState,
        location_id: &str,
    ) -> Option<&'a WorldMapNode> {
        let node_ids = self.map_node_ids_by_location.get(location_id)?;
        for node_id in node_ids {
            if let Some(node) = world.world_map_nodes.get(node_id) {
                return Some(node);
            }
        }
        None
    }

    pub(super) fn map_node_for_location_with_tags<'a>(
        &self,
        world: &'a WorldState,
        location_id: &str,
        desired_tags: &[&str],
    ) -> Option<&'a WorldMapNode> {
        let node_ids = self.map_node_ids_by_location.get(location_id)?;
        for node_id in node_ids {
            let Some(node) = world.world_map_nodes.get(node_id) else {
                continue;
            };
            if node
                .interaction_tags
                .iter()
                .any(|tag| desired_tags.iter().any(|desired| tag == desired))
            {
                return Some(node);
            }
        }
        None
    }

    pub(super) fn work_order_index(&self, work_order_id: &str) -> Option<usize> {
        self.work_order_index_by_id.get(work_order_id).copied()
    }

    pub(super) fn latest_contract_index_for_actor(&self, matrix_user_id: &str) -> Option<usize> {
        self.latest_completable_contract_index_by_actor
            .get(matrix_user_id)
            .or_else(|| self.latest_contract_index_by_actor.get(matrix_user_id))
            .copied()
    }

    pub(super) fn latest_asset_index_for_owner(&self, matrix_user_id: &str) -> Option<usize> {
        self.latest_asset_index_by_owner
            .get(matrix_user_id)
            .copied()
    }

    pub(super) fn latest_company_index_for_owner(&self, matrix_user_id: &str) -> Option<usize> {
        self.latest_operating_company_index_by_owner
            .get(matrix_user_id)
            .or_else(|| self.latest_company_index_by_owner.get(matrix_user_id))
            .copied()
    }

    pub(super) fn resolve_work_order_index(
        &self,
        work_order_id: &str,
        matrix_user_id: &str,
        latest_by_actor: &HashMap<String, usize>,
    ) -> Option<usize> {
        if work_order_id == "latest" {
            latest_by_actor.get(matrix_user_id).copied()
        } else {
            self.work_order_index_by_id.get(work_order_id).copied()
        }
    }

    pub(super) fn resolve_deliverable_work_order_index(
        &self,
        work_order_id: &str,
        matrix_user_id: &str,
    ) -> Option<usize> {
        self.resolve_work_order_index(
            work_order_id,
            matrix_user_id,
            &self.latest_deliverable_work_order_by_seller,
        )
    }

    pub(super) fn resolve_acceptable_work_order_index(
        &self,
        work_order_id: &str,
        matrix_user_id: &str,
    ) -> Option<usize> {
        self.resolve_work_order_index(
            work_order_id,
            matrix_user_id,
            &self.latest_acceptable_work_order_by_buyer,
        )
    }

    pub(super) fn resolve_rejectable_work_order_index(
        &self,
        work_order_id: &str,
        matrix_user_id: &str,
    ) -> Option<usize> {
        self.resolve_work_order_index(
            work_order_id,
            matrix_user_id,
            &self.latest_rejectable_work_order_by_buyer,
        )
    }

    pub(super) fn resolve_reopenable_work_order_index(
        &self,
        work_order_id: &str,
        matrix_user_id: &str,
    ) -> Option<usize> {
        self.resolve_work_order_index(
            work_order_id,
            matrix_user_id,
            &self.latest_reopenable_work_order_by_buyer,
        )
    }

    pub(super) fn resolve_cancellable_work_order_index(
        &self,
        work_order_id: &str,
        matrix_user_id: &str,
    ) -> Option<usize> {
        self.resolve_work_order_index(
            work_order_id,
            matrix_user_id,
            &self.latest_cancellable_work_order_by_buyer,
        )
    }

    pub(super) fn faction_standing_index(
        &self,
        matrix_user_id: &str,
        faction_id: &str,
    ) -> Option<usize> {
        self.faction_standing_index_by_user_faction
            .get(&(matrix_user_id.to_string(), faction_id.to_string()))
            .copied()
    }

    pub(super) fn replace_purchase_by_id(&self, world: &mut WorldState, purchase: &WorldPurchase) {
        if let Some(index) = self
            .purchase_index_by_id
            .get(&purchase.purchase_id)
            .copied()
        {
            if let Some(stored_purchase) = world.world_purchases.get_mut(index) {
                *stored_purchase = purchase.clone();
            }
        }
    }

    pub(super) fn replace_contract_by_id(&self, world: &mut WorldState, contract: &WorldContract) {
        if let Some(index) = self.contract_index(&contract.contract_id) {
            if let Some(stored_contract) = world.world_contracts.get_mut(index) {
                *stored_contract = contract.clone();
            }
        }
    }

    pub(super) fn replace_contract_completion_by_id(
        &self,
        world: &mut WorldState,
        completion: &WorldContractCompletion,
    ) {
        if let Some(index) = self
            .contract_completion_index_by_id
            .get(&completion.completion_id)
            .copied()
        {
            if let Some(stored_completion) = world.world_contract_completions.get_mut(index) {
                *stored_completion = completion.clone();
            }
        }
    }

    pub(super) fn replace_work_order_by_id(
        &self,
        world: &mut WorldState,
        work_order: &WorldWorkOrder,
    ) {
        if let Some(index) = self
            .work_order_index_by_id
            .get(&work_order.work_order_id)
            .copied()
        {
            if let Some(stored_work_order) = world.world_work_orders.get_mut(index) {
                *stored_work_order = work_order.clone();
            }
        }
    }

    pub(super) fn replace_acceptance_by_id(
        &self,
        world: &mut WorldState,
        acceptance: &WorldWorkAcceptance,
    ) {
        if let Some(index) = self
            .acceptance_index_by_id
            .get(&acceptance.acceptance_id)
            .copied()
        {
            if let Some(stored_acceptance) = world.world_work_acceptances.get_mut(index) {
                *stored_acceptance = acceptance.clone();
            }
        }
    }

    pub(super) fn replace_rejection_by_id(
        &self,
        world: &mut WorldState,
        rejection: &WorldWorkRejection,
    ) {
        if let Some(index) = self
            .rejection_index_by_id
            .get(&rejection.rejection_id)
            .copied()
        {
            if let Some(stored_rejection) = world.world_work_rejections.get_mut(index) {
                *stored_rejection = rejection.clone();
            }
        }
    }

    pub(super) fn replace_reopen_by_id(&self, world: &mut WorldState, reopen: &WorldWorkReopen) {
        if let Some(index) = self.reopen_index_by_id.get(&reopen.reopen_id).copied() {
            if let Some(stored_reopen) = world.world_work_reopens.get_mut(index) {
                *stored_reopen = reopen.clone();
            }
        }
    }

    pub(super) fn replace_cancellation_by_id(
        &self,
        world: &mut WorldState,
        cancellation: &WorldWorkCancellation,
    ) {
        if let Some(index) = self
            .cancellation_index_by_id
            .get(&cancellation.cancellation_id)
            .copied()
        {
            if let Some(stored_cancellation) = world.world_work_cancellations.get_mut(index) {
                *stored_cancellation = cancellation.clone();
            }
        }
    }
}
