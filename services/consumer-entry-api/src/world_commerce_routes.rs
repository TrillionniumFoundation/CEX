use super::*;

const TRILLIONNIUM_MARKET_SIMULATOR_CONTRACT_VERSION: &str = "trillionnium_market_simulator_v1";

/// Push a world economy event at most once.  World command handlers can be retried after a
/// remote Ledger call has already completed; the event id is the durable projection key and must
/// therefore be checked before mutating the append-only compatibility feed.
fn push_world_economy_event_once(world: &mut WorldState, event: WorldEconomyEvent) -> bool {
    if world
        .world_economy_events
        .iter()
        .any(|existing| existing.economy_event_id == event.economy_event_id)
    {
        return false;
    }
    world.world_economy_events.push(event);
    true
}

fn world_rejection_attempt(world: &WorldState, work_order_id: &str) -> usize {
    world
        .world_work_rejections
        .iter()
        .filter(|rejection| rejection.work_order_id == work_order_id)
        .count()
}

fn world_reopen_attempt(world: &WorldState, work_order_id: &str) -> usize {
    world
        .world_work_reopens
        .iter()
        .filter(|reopen| reopen.work_order_id == work_order_id)
        .count()
}

fn world_cancellation_attempt(world: &WorldState, work_order_id: &str) -> usize {
    world
        .world_work_cancellations
        .iter()
        .filter(|cancellation| cancellation.work_order_id == work_order_id)
        .count()
}

fn world_market_tax_credits_for_price(price_credits: i64) -> i64 {
    (price_credits.max(1) / 20).max(1)
}

fn world_seller_net_credits_for_price(price_credits: i64) -> i64 {
    price_credits
        .max(1)
        .saturating_sub(world_market_tax_credits_for_price(price_credits))
        .max(0)
}

/// A compatibility economy event is useful as a projection marker only when its immutable
/// identity tuple is still intact.  Looking up an event by id alone is insufficient: pre-v12
/// snapshots can contain a stale/corrupt marker (or a marker written before the typed receipt
/// cutover), and that marker must never authorize a value-bearing retry.
fn world_economy_event_matches(
    event: &WorldEconomyEvent,
    event_id: &str,
    matrix_user_id: &str,
    event_kind: &str,
    subject_id: &str,
    credits_delta: i64,
    reputation_delta: i64,
    created_at_epoch: i64,
) -> bool {
    event.economy_event_id == event_id
        && event.matrix_user_id == matrix_user_id
        && event.event_kind == event_kind
        && event.subject_id == subject_id
        && event.credits_delta == credits_delta
        && event.reputation_delta == reputation_delta
        && event.created_at_epoch == created_at_epoch
}

/// Return whether both immutable buyer-reserve and seller-settlement receipts authorize the
/// purchase projection.  The legacy economy event is deliberately not consulted here; it is only
/// an idempotent projection marker after the exact receipt pair has been proven.
fn world_purchase_payment_receipts_active(world: &WorldState, purchase: &WorldPurchase) -> bool {
    world_purchase_buyer_reserve_active(world, purchase)
        && world_purchase_seller_settlement_active(world, purchase)
}

/// Resolve a purchase economy marker without treating the marker itself as Ledger authority.
/// `Some(true)` means the exact receipt pair and event tuple match, `Some(false)` means an event
/// with the deterministic id exists but is poisoned, and `None` means no marker exists yet.
pub(super) fn world_purchase_projection_marker(
    world: &WorldState,
    purchase: &WorldPurchase,
    event_id: &str,
    matrix_user_id: &str,
    event_kind: &str,
    credits_delta: i64,
    reputation_delta: i64,
) -> Option<bool> {
    let event = world
        .world_economy_events
        .iter()
        .find(|event| event.economy_event_id == event_id)?;
    let matches = world_purchase_payment_receipts_active(world, purchase)
        && world_economy_event_matches(
            event,
            event_id,
            matrix_user_id,
            event_kind,
            &purchase.purchase_id,
            credits_delta,
            reputation_delta,
            purchase.created_at_epoch,
        );
    Some(matches)
}

/// Work delivery identity is derived from the immutable command tuple and the current reopen
/// cycle.  Wall-clock seconds are deliberately excluded: a response-loss retry must address the
/// same delivery row, while a later delivery after a reopen gets a fresh cycle identity even when
/// the seller happens to submit identical text.
pub(super) fn world_work_delivery_id(
    work_order_id: &str,
    matrix_user_id: &str,
    reopen_attempt: usize,
    body: &str,
) -> String {
    league_hash_id(
        "world-delivery",
        &format!("{work_order_id}:{matrix_user_id}:{reopen_attempt}:{body}"),
    )
}

pub(super) fn legacy_world_work_delivery_id(
    work_order_id: &str,
    matrix_user_id: &str,
    created_at_epoch: i64,
) -> String {
    league_hash_id(
        "world-delivery",
        &format!("{work_order_id}:{matrix_user_id}:{created_at_epoch}"),
    )
}

fn world_work_delivery_tuple_matches(
    delivery: &WorldWorkDelivery,
    work_order_id: &str,
    matrix_user_id: &str,
    body: &str,
) -> bool {
    delivery.work_order_id == work_order_id
        && delivery.matrix_user_id == matrix_user_id
        && delivery.body == body
}

/// Find an exact delivery replay.  New rows use the deterministic identity above; the validated
/// legacy branch lets an upgraded pre-v12 row be replayed without trusting an arbitrary matching
/// body/id pair.  A legacy row is only adopted while the order is in a delivery terminal/hold
/// state; after a reopen the cycle counter forces a new deterministic identity.
pub(super) fn world_work_delivery_for_request<'a>(
    world: &'a WorldState,
    work_order_id: &str,
    matrix_user_id: &str,
    reopen_attempt: usize,
    body: &str,
    work_order_status: &str,
) -> Option<&'a WorldWorkDelivery> {
    let deterministic_id =
        world_work_delivery_id(work_order_id, matrix_user_id, reopen_attempt, body);
    if let Some(delivery) = world
        .world_work_deliveries
        .iter()
        .find(|delivery| delivery.delivery_id == deterministic_id)
    {
        return world_work_delivery_tuple_matches(delivery, work_order_id, matrix_user_id, body)
            .then_some(delivery);
    }
    if !matches!(work_order_status, "delivered" | "delivery_review_hold") {
        return None;
    }
    world
        .world_work_deliveries
        .iter()
        .filter(|delivery| {
            world_work_delivery_tuple_matches(delivery, work_order_id, matrix_user_id, body)
                && delivery.delivery_id
                    == legacy_world_work_delivery_id(
                        work_order_id,
                        matrix_user_id,
                        delivery.created_at_epoch,
                    )
        })
        .max_by(|left, right| {
            // Prefer a terminal replay, then the latest persisted attempt.  This ordering is
            // deterministic even when a legacy snapshot contains duplicate rows.
            (left.status == "delivered")
                .cmp(&(right.status == "delivered"))
                .then_with(|| left.created_at_epoch.cmp(&right.created_at_epoch))
                .then_with(|| left.delivery_id.cmp(&right.delivery_id))
        })
}

fn world_work_delivery_event_id(delivery: &WorldWorkDelivery) -> String {
    league_hash_id(
        "world-econ",
        &format!(
            "{}:{}:{}",
            delivery.matrix_user_id, delivery.work_order_id, delivery.delivery_id
        ),
    )
}

fn legacy_world_work_delivery_event_id(delivery: &WorldWorkDelivery) -> String {
    league_hash_id(
        "world-econ",
        &format!(
            "{}:{}:{}",
            delivery.matrix_user_id, delivery.work_order_id, delivery.created_at_epoch
        ),
    )
}

pub(super) fn world_acceptance_projection_marker(
    world: &WorldState,
    purchase: &WorldPurchase,
    event_id: &str,
    matrix_user_id: &str,
    work_order_id: &str,
    reputation_delta: i64,
    acceptance_created_at_epoch: i64,
) -> Option<bool> {
    let event = world
        .world_economy_events
        .iter()
        .find(|event| event.economy_event_id == event_id)?;
    let matches = world_purchase_buyer_consume_completed(world, purchase)
        && world_economy_event_matches(
            event,
            event_id,
            matrix_user_id,
            "work_accepted",
            work_order_id,
            0,
            reputation_delta,
            acceptance_created_at_epoch,
        );
    Some(matches)
}

/// Return the exact buyer-side amount that a purchase reserve/consume/refund is allowed to use.
///
/// `WorldPurchase.price_credits` is retained as a compatibility field, but only a positive
/// integer is a value-bearing Ledger authority.  A non-positive value is represented by an
/// explicit terminal skip and must never unlock a buyer progression gate.
fn world_purchase_buyer_amount(purchase: &WorldPurchase) -> Option<i64> {
    (purchase.price_credits > 0).then_some(purchase.price_credits)
}

/// Return the exact seller-side amount for settlement/chargeback operations.  A zero seller net
/// is a valid no-value terminal outcome, not a progression-bearing settlement.
fn world_purchase_seller_amount(purchase: &WorldPurchase) -> i64 {
    world_seller_net_credits_for_price(purchase.price_credits)
}

fn world_receipt_allows_exact_operation(
    receipt: &TermExchangeReceiptState,
    expected_amount_credits: i64,
    expected_term_id: &str,
    allowed_statuses: &[&str],
) -> bool {
    if expected_amount_credits <= 0
        || receipt.term_id != expected_term_id
        || receipt.backend_id != term_exchange_protocol::CEX_SETTLEMENT_BACKEND_ID
        || receipt.backend_kind != term_exchange_protocol::SettlementBackendKind::Cex
        || receipt.progression_class
            != term_exchange_protocol::ReceiptProgressionClass::ProgressionAllowed
        || receipt.amount_credits != Some(expected_amount_credits)
    {
        return false;
    }
    serde_json::to_value(&receipt.status)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .is_some_and(|status| allowed_statuses.iter().any(|allowed| *allowed == status))
}

/// A seller chargeback may be skipped only when the deterministic seller net is zero and the
/// receipt/status explicitly says `skipped_zero_seller_net`.  Generic `skipped_*` statuses are
/// holds or precondition failures and must never clear a rejection/cancellation.
fn world_receipt_is_zero_seller_net_skip(
    receipt: &TermExchangeReceiptState,
    expected_amount_credits: i64,
) -> bool {
    expected_amount_credits == 0
        && receipt.term_id == "world_commerce_purchase"
        && receipt.backend_id == term_exchange_protocol::CEX_SETTLEMENT_BACKEND_ID
        && receipt.backend_kind == term_exchange_protocol::SettlementBackendKind::Cex
        && receipt.progression_class
            == term_exchange_protocol::ReceiptProgressionClass::TerminalSkip
        && receipt.status == term_exchange_protocol::ReceiptStatus::SkippedZeroSellerNet
        && receipt.amount_credits == Some(0)
}

pub(super) fn world_settlement_is_zero_seller_net_skip(
    settlement: &LeagueLedgerSettlement,
    expected_amount_credits: i64,
) -> bool {
    if expected_amount_credits != 0 || !settlement.terminal_skip() {
        return false;
    }
    if let Some(receipt) = settlement.term_exchange_receipt.as_ref() {
        return world_receipt_is_zero_seller_net_skip(receipt, expected_amount_credits);
    }
    settlement.status == "skipped_zero_seller_net" && settlement.amount_credits == Some(0)
}

pub(super) fn world_settlement_is_zero_reward_skip(
    settlement: &LeagueLedgerSettlement,
    expected_amount_credits: Option<i64>,
) -> bool {
    if expected_amount_credits != Some(0)
        || !settlement.terminal_skip()
        || settlement.status != "skipped_zero_reward"
        || settlement.amount_credits != Some(0)
    {
        return false;
    }
    if let Some(receipt) = settlement.term_exchange_receipt.as_ref() {
        return receipt.term_id == "world_contract_completion_settlement"
            && receipt.backend_id == term_exchange_protocol::CEX_SETTLEMENT_BACKEND_ID
            && receipt.backend_kind == term_exchange_protocol::SettlementBackendKind::Cex
            && receipt.progression_class
                == term_exchange_protocol::ReceiptProgressionClass::TerminalSkip
            && receipt.status == term_exchange_protocol::ReceiptStatus::SkippedZeroReward
            && receipt.amount_credits == Some(0);
    }
    true
}

/// Project a contract completion's Ledger outcome into retry-safe World status fields.
///
/// A mutable `completed_*` string is never sufficient evidence of a payout. Only an exact
/// progression receipt (or an explicitly typed zero-reward skip) may enter a terminal completed
/// state; every other outcome remains retryable/reconcilable.
pub(super) fn world_contract_settlement_projection(
    current_status: &str,
    ledger_status: Option<&str>,
    settlement_completed: bool,
    zero_reward_skipped: bool,
) -> (String, String) {
    if settlement_completed {
        return ("completed_settled".to_string(), "completed".to_string());
    }
    if zero_reward_skipped {
        return (
            "completed_no_reward".to_string(),
            "completed_no_reward".to_string(),
        );
    }
    match ledger_status {
        Some("held_review") => ("review_hold".to_string(), "review_hold".to_string()),
        Some("settled") | Some("duplicate") => (
            "settlement_reconcile_required".to_string(),
            "reconcile_required".to_string(),
        ),
        Some(_) => (
            "settlement_blocked".to_string(),
            "settlement_blocked".to_string(),
        ),
        None => (current_status.to_string(), "settlement_pending".to_string()),
    }
}

pub(super) fn world_purchase_seller_settlement_active(
    world: &WorldState,
    purchase: &WorldPurchase,
) -> bool {
    let reopen_settlement_intent_prefix =
        format!("world_purchase_reopen_settlement:{}:", purchase.purchase_id);
    if let Some(receipt) = latest_world_term_exchange_receipt_for_intent_prefix(
        world,
        &reopen_settlement_intent_prefix,
    ) {
        let expected = world_purchase_seller_amount(purchase);
        return world_receipt_allows_exact_operation(
            receipt,
            expected,
            "world_commerce_purchase",
            &["settled", "duplicate"],
        ) || world_receipt_is_zero_seller_net_skip(receipt, expected);
    }
    let settlement_intent_id = format!("world_purchase:grant:{}", purchase.purchase_id);
    if let Some(receipt) = world_term_exchange_receipt_for_intent(world, &settlement_intent_id) {
        let expected = world_purchase_seller_amount(purchase);
        return world_receipt_allows_exact_operation(
            receipt,
            expected,
            "world_commerce_purchase",
            &["settled", "duplicate"],
        ) || world_receipt_is_zero_seller_net_skip(receipt, expected);
    }
    // Legacy status strings do not carry an authenticated amount.  They remain useful for
    // diagnostics/read compatibility, but cannot unlock a value-bearing world action after the
    // exact receipt cutover.
    world_purchase_seller_amount(purchase) == 0
        && purchase.ledger_status.as_deref() == Some("skipped_zero_seller_net")
}

pub(super) fn world_purchase_buyer_reserve_active(
    world: &WorldState,
    purchase: &WorldPurchase,
) -> bool {
    let reserve_intent_id = format!("world_purchase:reserve:{}", purchase.purchase_id);
    if let Some(receipt) = world_term_exchange_receipt_for_intent(world, &reserve_intent_id) {
        return world_purchase_buyer_amount(purchase).is_some_and(|expected| {
            world_receipt_allows_exact_operation(
                receipt,
                expected,
                "world_commerce_purchase",
                &["reserved", "duplicate"],
            )
        });
    }
    let reopen_reserve_intent_prefix =
        format!("world_purchase_reopen_reserve:{}:", purchase.purchase_id);
    if let Some(receipt) =
        latest_world_term_exchange_receipt_for_intent_prefix(world, &reopen_reserve_intent_prefix)
    {
        return world_purchase_buyer_amount(purchase).is_some_and(|expected| {
            world_receipt_allows_exact_operation(
                receipt,
                expected,
                "world_commerce_purchase",
                &["reserved", "duplicate"],
            )
        });
    }
    false
}

pub(super) fn world_purchase_buyer_consume_completed(
    world: &WorldState,
    purchase: &WorldPurchase,
) -> bool {
    let consume_intent_id = format!("world_purchase:consume:{}", purchase.purchase_id);
    if let Some(receipt) = world_term_exchange_receipt_for_intent(world, &consume_intent_id) {
        return world_purchase_buyer_amount(purchase).is_some_and(|expected| {
            world_receipt_allows_exact_operation(
                receipt,
                expected,
                "world_commerce_purchase",
                &["consumed", "duplicate"],
            )
        });
    }
    false
}

fn world_purchase_buyer_refund_receipt<'a>(
    world: &'a WorldState,
    purchase: &WorldPurchase,
    refund_scope: Option<&str>,
) -> Option<&'a TermExchangeReceiptState> {
    if let Some(scope) = refund_scope {
        let refund_intent_id = format!("world_purchase:refund:{}:{}", purchase.purchase_id, scope);
        return world_term_exchange_receipt_for_intent(world, &refund_intent_id);
    }
    let refund_intent_prefix = format!("world_purchase:refund:{}:", purchase.purchase_id);
    latest_world_term_exchange_receipt_for_intent_prefix(world, &refund_intent_prefix)
}

pub(super) fn world_purchase_buyer_refund_completed(
    world: &WorldState,
    purchase: &WorldPurchase,
    refund_scope: Option<&str>,
) -> bool {
    world_purchase_buyer_amount(purchase).is_some_and(|expected| {
        world_purchase_buyer_refund_receipt(world, purchase, refund_scope).is_some_and(|receipt| {
            world_receipt_allows_exact_operation(
                receipt,
                expected,
                "world_commerce_purchase",
                &["refunded", "duplicate"],
            )
        })
    })
}

/// Reuse a previously persisted buyer-refund receipt when a rejection/cancellation is retried
/// only for the seller chargeback leg.  The old implementation synthesized a `refunded` status
/// with no amount or receipt, which let a retry mint a progression decision from compatibility
/// fields alone.  A retry is valid only when the immutable typed receipt proves the exact price.
fn world_replay_buyer_refund_settlement(
    world: &WorldState,
    purchase: &WorldPurchase,
    refund_scope: &str,
) -> Option<LeagueLedgerSettlement> {
    let expected_amount_credits = world_purchase_buyer_amount(purchase)?;
    let receipt = world_purchase_buyer_refund_receipt(world, purchase, Some(refund_scope))?;
    if !world_receipt_allows_exact_operation(
        receipt,
        expected_amount_credits,
        "world_commerce_purchase",
        &["refunded", "duplicate"],
    ) {
        return None;
    }
    Some(LeagueLedgerSettlement {
        status: "refunded".to_string(),
        account_id: purchase.buyer_ledger_account_id.clone(),
        entry_id: receipt
            .ledger_entry_id
            .clone()
            .or_else(|| purchase.buyer_consume_entry_id.clone()),
        balance_after: purchase.buyer_consume_balance_after,
        error: None,
        amount_credits: receipt.amount_credits,
        term_exchange_receipt: Some(receipt.clone()),
    })
}

pub(super) fn world_purchase_seller_chargeback_cleared(
    world: &WorldState,
    purchase: &WorldPurchase,
    chargeback_scope: Option<&str>,
) -> bool {
    if let Some(scope) = chargeback_scope {
        let consume_intent_id = format!(
            "world_purchase_seller_chargeback_consume:{}:{}",
            purchase.purchase_id, scope
        );
        if let Some(receipt) = world_term_exchange_receipt_for_intent(world, &consume_intent_id) {
            let expected = world_purchase_seller_amount(purchase);
            return world_receipt_allows_exact_operation(
                receipt,
                expected,
                "world_commerce_purchase",
                &["seller_chargeback_consumed", "duplicate"],
            ) || world_receipt_is_zero_seller_net_skip(receipt, expected);
        }
    } else {
        let consume_intent_prefix = format!(
            "world_purchase_seller_chargeback_consume:{}:",
            purchase.purchase_id
        );
        if let Some(receipt) =
            latest_world_term_exchange_receipt_for_intent_prefix(world, &consume_intent_prefix)
        {
            let expected = world_purchase_seller_amount(purchase);
            return world_receipt_allows_exact_operation(
                receipt,
                expected,
                "world_commerce_purchase",
                &["seller_chargeback_consumed", "duplicate"],
            ) || world_receipt_is_zero_seller_net_skip(receipt, expected);
        }
    }
    world_purchase_seller_amount(purchase) == 0
        && purchase.ledger_status.as_deref() == Some("skipped_zero_seller_net")
}

fn world_purchase_seller_chargeback_recoverable_hold(
    world: &WorldState,
    purchase: &WorldPurchase,
    chargeback_scope: &str,
) -> bool {
    let consume_intent_id = format!(
        "world_purchase_seller_chargeback_consume:{}:{}",
        purchase.purchase_id, chargeback_scope
    );
    let reserve_intent_id = format!(
        "world_purchase_seller_chargeback_reserve:{}:{}",
        purchase.purchase_id, chargeback_scope
    );
    [consume_intent_id, reserve_intent_id]
        .iter()
        .filter_map(|intent_id| world_term_exchange_receipt_for_intent(world, intent_id))
        .max_by(|left, right| {
            left.finalized_at_epoch
                .cmp(&right.finalized_at_epoch)
                .then_with(|| left.receipt_id.cmp(&right.receipt_id))
        })
        .map(|receipt| {
            receipt.term_id == "world_commerce_purchase"
                && receipt.backend_id == term_exchange_protocol::CEX_SETTLEMENT_BACKEND_ID
                && receipt.backend_kind == term_exchange_protocol::SettlementBackendKind::Cex
                && receipt.progression_class
                    == term_exchange_protocol::ReceiptProgressionClass::RecoverableHold
                && receipt.amount_credits == Some(world_purchase_seller_amount(purchase))
        })
        .unwrap_or(false)
}

pub(super) fn world_purchase_rejection_settlement_released(
    world: &WorldState,
    purchase: &WorldPurchase,
    work_order_id: &str,
) -> bool {
    let rejection_scope = latest_world_rejection_scope_for_work_order(world, work_order_id);
    purchase.status == "rejected_refunded"
        && world_purchase_buyer_refund_completed(world, purchase, rejection_scope.as_deref())
        && world_purchase_seller_chargeback_cleared(world, purchase, rejection_scope.as_deref())
}

pub(super) fn world_work_reopen_reserve_completed(
    world: &WorldState,
    reopen: &WorldWorkReopen,
) -> bool {
    if let Some(purchase) = world_purchase_for_work_order(world, &reopen.work_order_id) {
        let reopen_reserve_intent_prefix =
            format!("world_purchase_reopen_reserve:{}:", purchase.purchase_id);
        if let Some(receipt) = latest_world_term_exchange_receipt_for_intent_prefix(
            world,
            &reopen_reserve_intent_prefix,
        ) {
            return world_purchase_buyer_amount(purchase).is_some_and(|expected| {
                world_receipt_allows_exact_operation(
                    receipt,
                    expected,
                    "world_commerce_purchase",
                    &["reserved", "duplicate"],
                )
            });
        }
    }
    false
}

fn world_purchase_for_work_order<'a>(
    world: &'a WorldState,
    work_order_id: &str,
) -> Option<&'a WorldPurchase> {
    let work_order = world
        .world_work_orders
        .iter()
        .find(|work_order| work_order.work_order_id == work_order_id)?;
    world
        .world_purchases
        .iter()
        .find(|purchase| purchase.purchase_id == work_order.purchase_id)
}

fn latest_world_rejection_scope_for_work_order(
    world: &WorldState,
    work_order_id: &str,
) -> Option<String> {
    world
        .world_work_rejections
        .iter()
        .rev()
        .find(|rejection| rejection.work_order_id == work_order_id)
        .map(|rejection| rejection.rejection_id.clone())
}

fn latest_world_cancellation_scope_for_work_order(
    world: &WorldState,
    work_order_id: &str,
) -> Option<String> {
    world
        .world_work_cancellations
        .iter()
        .rev()
        .find(|cancellation| cancellation.work_order_id == work_order_id)
        .map(|cancellation| cancellation.cancellation_id.clone())
}

pub(super) fn world_work_rejection_refund_completed(
    world: &WorldState,
    rejection: &WorldWorkRejection,
) -> bool {
    let Some(purchase) = world_purchase_for_work_order(world, &rejection.work_order_id) else {
        return false;
    };
    world_purchase_buyer_refund_completed(world, purchase, Some(&rejection.rejection_id))
}

pub(super) fn world_work_cancellation_refund_completed(
    world: &WorldState,
    cancellation: &WorldWorkCancellation,
) -> bool {
    let Some(purchase) = world_purchase_for_work_order(world, &cancellation.work_order_id) else {
        return false;
    };
    world_purchase_buyer_refund_completed(world, purchase, Some(&cancellation.cancellation_id))
}

fn world_term_exchange_receipt_for_intent<'a>(
    world: &'a WorldState,
    intent_id: &str,
) -> Option<&'a TermExchangeReceiptState> {
    let receipt_id = format!("receipt:{intent_id}");
    if let Some(canonical) = world.world_term_exchange_receipts.get(&receipt_id) {
        // A canonical receipt key is an immutable identity binding.  Never fall back to another
        // row when that key has been poisoned with a different intent; doing so could authorize
        // a value-bearing operation from unrelated evidence.
        if canonical.intent_id != intent_id || canonical.receipt_id != receipt_id {
            return None;
        }
    }
    world
        .world_term_exchange_receipts
        .values()
        // The receipt id is part of the immutable operation identity.  A row with the
        // right intent but a non-canonical receipt id is legacy/corrupt evidence and must
        // not be selected as a value-authorizing fallback (especially when the canonical
        // map key has been poisoned or removed).
        .filter(|receipt| receipt.intent_id == intent_id && receipt.receipt_id == receipt_id)
        .max_by(|left, right| {
            left.finalized_at_epoch
                .cmp(&right.finalized_at_epoch)
                .then_with(|| left.receipt_id.cmp(&right.receipt_id))
        })
}

fn latest_world_term_exchange_receipt_for_intent_prefix<'a>(
    world: &'a WorldState,
    intent_prefix: &str,
) -> Option<&'a TermExchangeReceiptState> {
    world
        .world_term_exchange_receipts
        .values()
        .filter(|receipt| {
            receipt.intent_id.starts_with(intent_prefix)
                && receipt.receipt_id == format!("receipt:{}", receipt.intent_id)
        })
        .max_by(|left, right| {
            left.finalized_at_epoch
                .cmp(&right.finalized_at_epoch)
                .then_with(|| left.receipt_id.cmp(&right.receipt_id))
        })
}

pub(super) fn world_contract_completion_released(
    world: &WorldState,
    completion: &WorldContractCompletion,
) -> bool {
    let Ok(expected_amount_credits) =
        whole_credits_from_compatibility_amount(completion.reward_amount)
    else {
        return false;
    };
    let intent_id = format!("world_contract_completion:{}", completion.completion_id);
    if let Some(receipt) = world_term_exchange_receipt_for_intent(world, &intent_id) {
        return world_receipt_allows_exact_operation(
            receipt,
            expected_amount_credits,
            "world_contract_completion_settlement",
            &["settled", "duplicate"],
        ) || (expected_amount_credits == 0
            && receipt.term_id == "world_contract_completion_settlement"
            && receipt.backend_id == term_exchange_protocol::CEX_SETTLEMENT_BACKEND_ID
            && receipt.backend_kind == term_exchange_protocol::SettlementBackendKind::Cex
            && receipt.progression_class
                == term_exchange_protocol::ReceiptProgressionClass::TerminalSkip
            && receipt.status == term_exchange_protocol::ReceiptStatus::SkippedZeroReward
            && receipt.amount_credits == Some(0));
    }
    false
}

/// Compare the immutable tuple bound to a deterministic contract-completion id.  A matching hash
/// with a different contract, actor, or report body is an idempotency collision and must never be
/// allowed to overwrite or settle the existing completion row.
pub(super) fn world_contract_completion_identity_matches(
    completion: &WorldContractCompletion,
    contract_id: &str,
    matrix_user_id: &str,
    body: &str,
) -> bool {
    completion.contract_id == contract_id
        && completion.matrix_user_id == matrix_user_id
        && completion.body == body
}

pub(super) fn world_contract_completion_id_collision_response(
    completion_id: &str,
    error: &'static str,
) -> Response {
    (
        StatusCode::CONFLICT,
        Json(json!({
            "error": error,
            "completion_id": completion_id,
        })),
    )
        .into_response()
}

/// Resolve a contract completion persisted by the pre-v12 routes.
///
/// Those routes derived completion ids from the wall-clock second.  The timestamp is no longer
/// available in a retry request, so look up only a row whose immutable contract/actor/body tuple
/// matches and whose id can be recomputed from its persisted creation timestamp.  Both the HTTP
/// world-contract endpoint and the Trillionnium tactics endpoint had distinct legacy prefixes;
/// accept either so an upgrade never creates a second Ledger intent for the same report.
fn legacy_world_contract_completion_for_prefixes(
    world: &WorldState,
    contract_id: &str,
    matrix_user_id: &str,
    body: &str,
    legacy_prefixes: &[&str],
) -> Option<WorldContractCompletion> {
    world
        .world_contract_completions
        .iter()
        .filter(|completion| {
            completion.contract_id == contract_id
                && completion.matrix_user_id == matrix_user_id
                && completion.body == body
        })
        .filter(|completion| {
            legacy_prefixes.iter().any(|prefix| {
                league_hash_id(
                    prefix,
                    &format!("{}:{}:{}", contract_id, completion.created_at_epoch, body),
                ) == completion.completion_id
            })
        })
        .max_by(|left, right| {
            world_contract_completion_replay_priority(world, left)
                .cmp(&world_contract_completion_replay_priority(world, right))
                .then_with(|| left.created_at_epoch.cmp(&right.created_at_epoch))
                .then_with(|| left.completion_id.cmp(&right.completion_id))
        })
        .cloned()
}

fn world_contract_completion_replay_priority(
    world: &WorldState,
    completion: &WorldContractCompletion,
) -> u8 {
    let intent_id = format!("world_contract_completion:{}", completion.completion_id);
    let receipt_priority = world
        .world_term_exchange_receipts
        .values()
        .filter(|receipt| {
            receipt.intent_id == intent_id && receipt.receipt_id == format!("receipt:{intent_id}")
        })
        .map(|receipt| match receipt.status {
            term_exchange_protocol::ReceiptStatus::Settled
            | term_exchange_protocol::ReceiptStatus::ApprovedRelease
            | term_exchange_protocol::ReceiptStatus::Duplicate
            | term_exchange_protocol::ReceiptStatus::SkippedZeroReward => 3,
            _ => 2,
        })
        .max()
        .unwrap_or(0);
    if receipt_priority > 0 {
        return receipt_priority;
    }
    match completion.ledger_status.as_deref() {
        Some("settled") | Some("approved_release") | Some("duplicate") => 1,
        _ => 0,
    }
}

/// Resolve either legacy completion prefix.  Kept as a compatibility helper for callers/tests
/// that do not know which historical route produced the row.
#[allow(dead_code)]
pub(super) fn legacy_world_contract_completion_for_request(
    world: &WorldState,
    contract_id: &str,
    matrix_user_id: &str,
    body: &str,
) -> Option<WorldContractCompletion> {
    legacy_world_contract_completion_for_prefixes(
        world,
        contract_id,
        matrix_user_id,
        body,
        &[
            "world-contract-completion",
            "world-trillionnium-task-completion",
        ],
    )
}

pub(super) fn legacy_world_contract_completion_for_legacy_prefix(
    world: &WorldState,
    contract_id: &str,
    matrix_user_id: &str,
    body: &str,
    legacy_prefix: &str,
) -> Option<WorldContractCompletion> {
    legacy_world_contract_completion_for_prefixes(
        world,
        contract_id,
        matrix_user_id,
        body,
        &[legacy_prefix],
    )
}

fn merge_world_contract_completion_optional_fields(
    target: &mut WorldContractCompletion,
    source: &WorldContractCompletion,
) {
    if target.ledger_status.is_none() {
        target.ledger_status = source.ledger_status.clone();
    }
    if target.ledger_account_id.is_none() {
        target.ledger_account_id = source.ledger_account_id.clone();
    }
    if target.ledger_entry_id.is_none() {
        target.ledger_entry_id = source.ledger_entry_id.clone();
    }
    if target.ledger_balance_after.is_none() {
        target.ledger_balance_after = source.ledger_balance_after;
    }
    if target.ledger_error.is_none() {
        target.ledger_error = source.ledger_error.clone();
    }
}

fn hydrate_world_contract_completion_from_receipt(
    world: &WorldState,
    completion: &mut WorldContractCompletion,
) {
    let intent_id = format!("world_contract_completion:{}", completion.completion_id);
    let Some(receipt) = world_term_exchange_receipt_for_intent(world, &intent_id) else {
        return;
    };
    let status = serde_json::to_value(receipt.status.clone())
        .ok()
        .and_then(|value| value.as_str().map(ToOwned::to_owned));
    if !matches!(
        completion.ledger_status.as_deref(),
        Some("settled") | Some("duplicate") | Some("skipped_zero_reward")
    ) {
        completion.ledger_status = status;
        completion.ledger_error = None;
    }
    if completion.ledger_entry_id.is_none() {
        completion.ledger_entry_id = receipt.ledger_entry_id.clone();
    }
}

/// Merge the result of a world-contract Ledger settlement into the compatibility projection.
///
/// Completion requests release the world lock while calling Ledger, so two requests can finish
/// in either order.  The old inline projection replaced the completion row and contract status
/// unconditionally; a late hold/failure could therefore downgrade a previously settled reward.
/// This helper serializes the final projection under the world lock, preserves an exact terminal
/// receipt, and applies the reward projection at most once via its economy-event id.
pub(super) fn merge_world_contract_completion_settlement(
    league: &mut LeagueState,
    contract: &WorldContract,
    incoming_completion: &WorldContractCompletion,
    settlement: &LeagueLedgerSettlement,
    settlement_completed: bool,
    settlement_zero_reward_skipped: bool,
) -> WorldContractCompletion {
    // Store the typed receipt first.  `record_world_term_exchange_receipt` is monotonic, so a
    // delayed hold cannot poison an already-final receipt for this intent.
    record_world_term_exchange_receipt(&mut league.world, settlement.term_exchange_receipt.clone());

    let existing_index = league
        .world
        .world_contract_completions
        .iter()
        .rposition(|existing| existing.completion_id == incoming_completion.completion_id);
    let existing_completion = existing_index
        .and_then(|index| league.world.world_contract_completions.get(index).cloned());
    let existing_terminal = existing_completion
        .as_ref()
        .is_some_and(|existing| world_contract_completion_released(&league.world, existing));

    let completion = if existing_terminal {
        let mut existing = existing_completion
            .expect("existing terminal completion must be present at its indexed position");
        // Keep the first terminal payload authoritative, but allow a response-loss retry to
        // hydrate missing Ledger metadata (entry id/status) from its receipt or duplicate reply.
        merge_world_contract_completion_optional_fields(&mut existing, incoming_completion);
        hydrate_world_contract_completion_from_receipt(&league.world, &mut existing);
        existing
    } else {
        incoming_completion.clone()
    };

    if let Some(index) = existing_index {
        if let Some(stored) = league.world.world_contract_completions.get_mut(index) {
            *stored = completion.clone();
        }
    } else {
        league
            .world
            .world_contract_completions
            .push(completion.clone());
    }

    let expected_reward_credits =
        whole_credits_from_compatibility_amount(completion.reward_amount).ok();
    // If a prior terminal receipt was already present, use it as the authority even when this
    // request received a transient hold.  Otherwise the current settlement flags determine the
    // transition.
    let canonical_terminal = existing_terminal
        || (settlement_completed || settlement_zero_reward_skipped)
            && world_contract_completion_released(&league.world, &completion);
    let canonical_zero_reward_skipped = canonical_terminal
        && expected_reward_credits == Some(0)
        && world_contract_completion_released(&league.world, &completion);
    let canonical_reward_settled = canonical_terminal && !canonical_zero_reward_skipped;
    // Tactics-created contracts predate the generic HTTP completion route and used the
    // `world-trillionnium-task-reward` projection id/shape.  Keep that marker authoritative when
    // recovering such a row through either route, while still recognizing the newer generic
    // `world-contract-reward` marker.  Without the dual lookup, a route hand-off could credit the
    // player twice even though the Ledger intent/receipt was already idempotent.
    let task_reward_projection = contract.task_id.starts_with("trillionnium-task:");
    let canonical_reward_event_id =
        league_hash_id("world-contract-reward", &completion.completion_id);
    let legacy_reward_event_id =
        league_hash_id("world-trillionnium-task-reward", &completion.completion_id);
    let canonical_reputation_delta = (completion.score / 8.0).round() as i64;
    let canonical_rating_delta = ((completion.score - 50.0) / 3.0).round() as i64;
    let legacy_reputation_delta = (completion.score / 12.0).round() as i64;
    let legacy_rating_delta = ((completion.score - 50.0) / 4.0).round() as i64;
    let canonical_marker = league.world.world_economy_events.iter().any(|event| {
        world_economy_event_matches(
            event,
            &canonical_reward_event_id,
            &completion.matrix_user_id,
            "world_contract_reward",
            &contract.contract_id,
            expected_reward_credits.unwrap_or_default(),
            canonical_reputation_delta,
            completion.created_at_epoch,
        )
    });
    let legacy_marker = league.world.world_economy_events.iter().any(|event| {
        world_economy_event_matches(
            event,
            &legacy_reward_event_id,
            &completion.matrix_user_id,
            "trillionnium_task_reward",
            &contract.contract_id,
            expected_reward_credits.unwrap_or_default(),
            legacy_reputation_delta,
            completion.created_at_epoch,
        )
    });
    let completion_reward_already_projected = canonical_marker || legacy_marker;
    let completion_reward_event_id =
        if (task_reward_projection && !canonical_marker) || legacy_marker {
            legacy_reward_event_id.clone()
        } else {
            canonical_reward_event_id.clone()
        };
    let projection_reputation_delta = if completion_reward_event_id == legacy_reward_event_id {
        legacy_reputation_delta
    } else {
        canonical_reputation_delta
    };
    let projection_rating_delta = if completion_reward_event_id == legacy_reward_event_id {
        legacy_rating_delta
    } else {
        canonical_rating_delta
    };
    let projection_event_kind = if completion_reward_event_id == legacy_reward_event_id {
        "trillionnium_task_reward"
    } else {
        "world_contract_reward"
    };

    if canonical_reward_settled
        && expected_reward_credits.is_some_and(|amount| amount > 0)
        && !completion_reward_already_projected
    {
        let amount_credits = expected_reward_credits
            .expect("canonical world contract reward must carry an exact amount");
        let matrix_user_id = completion.matrix_user_id.clone();
        if exact_credits_to_legacy_display(amount_credits).is_some() {
            let mut player = ensure_league_player(league, &matrix_user_id, None);
            if let Some(next) = checked_legacy_display_add(player.earned_credits, amount_credits) {
                player.earned_credits = next;
            }
            player.xp = player.xp.saturating_add(completion.score.round() as i64);
            player.reputation = player
                .reputation
                .saturating_add(projection_reputation_delta);
            player.rating = player.rating.saturating_add(projection_rating_delta);
            league
                .players_by_matrix_user
                .insert(matrix_user_id.clone(), player);
        }

        let indexes = build_world_indexes(&league.world);
        let asset_delta = (completion.score / 5.0).round() as i64;
        if let Some(asset_index) = indexes.latest_asset_index_for_owner(&matrix_user_id) {
            let asset = &mut league.world.world_assets[asset_index];
            asset.value_score = asset.value_score.saturating_add(asset_delta.max(1));
            asset.upgrade_points = asset.upgrade_points.saturating_add(asset_delta.max(1));
            asset.upgrade_level = asset
                .upgrade_level
                .max(1)
                .saturating_add((asset.upgrade_points / 60).max(0));
            asset.last_upgrade_kind = Some("contract_completion".to_string());
            asset.status = "upgraded_by_contract".to_string();
        } else {
            league.world.world_assets.push(WorldAsset {
                asset_id: league_hash_id("world-asset", &completion.completion_id),
                owner_matrix_user_id: matrix_user_id.clone(),
                location_id: contract.location_id.clone(),
                asset_kind: "contract_proof".to_string(),
                name: "World Contract Proof".to_string(),
                status: "active".to_string(),
                value_score: asset_delta.max(1),
                upgrade_level: 1,
                upgrade_points: asset_delta.max(1),
                last_upgrade_kind: Some("contract_completion".to_string()),
                created_at_epoch: completion.created_at_epoch,
            });
        }
        push_world_economy_event_once(
            &mut league.world,
            WorldEconomyEvent {
                economy_event_id: completion_reward_event_id,
                matrix_user_id,
                event_kind: projection_event_kind.to_string(),
                subject_id: contract.contract_id.clone(),
                credits_delta: amount_credits,
                reputation_delta: projection_reputation_delta,
                created_at_epoch: completion.created_at_epoch,
            },
        );
    }

    let indexes = build_world_indexes(&league.world);
    if let Some(contract_index) = indexes.contract_index(&contract.contract_id) {
        let mut stored_contract = league.world.world_contracts[contract_index].clone();
        let (projected_status, projected_cex_status) = if canonical_terminal {
            world_contract_settlement_projection(
                &stored_contract.status,
                completion.ledger_status.as_deref(),
                canonical_reward_settled,
                canonical_zero_reward_skipped,
            )
        } else {
            world_contract_settlement_projection(
                &stored_contract.status,
                completion.ledger_status.as_deref(),
                settlement_completed,
                settlement_zero_reward_skipped,
            )
        };
        stored_contract.status = projected_status;
        stored_contract.cex_status = Some(projected_cex_status);
        if canonical_reward_settled && !completion_reward_already_projected {
            let asset_delta = (completion.score / 5.0).round() as i64;
            stored_contract.value_score = stored_contract
                .value_score
                .saturating_add(asset_delta.max(1));
        }
        indexes.replace_contract_by_id(&mut league.world, &stored_contract);
    }

    completion
}

fn world_market_simulation_json(world: &WorldState, listing: &WorldListing, now: i64) -> Value {
    let base_price = listing.price_credits.max(1);
    let recent_window_seconds = 86_400;
    let recent_company_purchase_count = world
        .world_purchases
        .iter()
        .filter(|purchase| {
            purchase.company_id == listing.company_id
                && now.saturating_sub(purchase.created_at_epoch) <= recent_window_seconds
        })
        .count() as i64;
    let active_company_listing_count = world
        .world_listings
        .iter()
        .filter(|candidate| {
            candidate.company_id == listing.company_id && candidate.status == "listed"
        })
        .count() as i64;
    let demand_premium = (recent_company_purchase_count
        .saturating_mul(base_price)
        .saturating_div(20))
    .clamp(0, 50);
    let scarcity_premium = (3_i64
        .saturating_sub(active_company_listing_count)
        .max(0)
        .saturating_mul(2))
    .clamp(0, 12);
    let quality_premium = (listing.quality_score / 25).clamp(0, 20);
    let dynamic_price_credits = base_price
        .saturating_add(demand_premium)
        .saturating_add(scarcity_premium)
        .saturating_add(quality_premium)
        .max(1);
    let market_tax_credits = world_market_tax_credits_for_price(dynamic_price_credits);
    let seller_net_credits = world_seller_net_credits_for_price(dynamic_price_credits);
    let demand_index = 100_i64
        .saturating_add(recent_company_purchase_count.saturating_mul(8))
        .saturating_add(quality_premium)
        .clamp(50, 200);
    let scarcity_index = 100_i64
        .saturating_add(scarcity_premium.saturating_mul(5))
        .saturating_sub(active_company_listing_count.saturating_mul(2))
        .clamp(40, 180);
    json!({
        "contract_version": TRILLIONNIUM_MARKET_SIMULATOR_CONTRACT_VERSION,
        "status": "priced",
        "listing_id": listing.listing_id,
        "company_id": listing.company_id,
        "base_price_credits": base_price,
        "dynamic_price_credits": dynamic_price_credits,
        "demand_premium_credits": demand_premium,
        "scarcity_premium_credits": scarcity_premium,
        "quality_premium_credits": quality_premium,
        "market_tax_credits": market_tax_credits,
        "seller_net_credits": seller_net_credits,
        "demand_index": demand_index,
        "scarcity_index": scarcity_index,
        "recent_company_purchase_count": recent_company_purchase_count,
        "active_company_listing_count": active_company_listing_count,
        "sinks": ["market_tax", "review_hold_delay", "refund_risk"],
        "strategy_hint": "High demand raises price; scarce quality supply earns more but pays a visible market tax.",
    })
}

pub(super) async fn get_world_assets(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }
    let league = state.inner.league_state.lock().await;
    let WorldRouteArtifacts {
        preview: route_preview,
        task_graph: route_task_graph,
        ..
    } = build_world_route_artifacts(&league.world);
    (
        StatusCode::OK,
        Json(json!({
            "kind": "trillionnium_world_assets",
            "world": "trillionnium_world",
            "assets": league.world.world_assets,
            "upgrades": league.world.world_asset_upgrades,
            "route_preview": route_preview,
            "route_task_graph": route_task_graph,
        })),
    )
        .into_response()
}

pub(super) async fn upgrade_world_asset_inner(
    state: AppState,
    asset_id: String,
    payload: WorldAssetUpgradeRequest,
) -> Response {
    let matrix_user_id = match normalize_league_matrix_user(&payload.matrix_user_id) {
        Some(value) => value,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "matrix_user_id is required" })),
            )
                .into_response()
        }
    };
    let body = match validate_text_payload(&payload.body, state.config().max_text_chars) {
        Ok(value) => value,
        Err(response) => return response,
    };
    let _room_id = payload.room_id.clone();
    let resolved_asset_id = {
        let league = state.inner.league_state.lock().await;
        let indexes = build_world_indexes(&league.world);
        let Some(asset_index) = indexes.resolve_asset_index(&asset_id, &matrix_user_id) else {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "no world asset found for player", "asset_id": asset_id })),
            )
                .into_response();
        };
        league
            .world
            .world_assets
            .get(asset_index)
            .map(|asset| asset.asset_id.clone())
            .unwrap_or_else(|| asset_id.clone())
    };
    let judgement =
        judge_league_submission_with_pipeline(&state, &body, "world_asset_upgrade").await;
    let snapshot = {
        let now = Utc::now().timestamp();
        let mut league = state.inner.league_state.lock().await;
        let indexes = build_world_indexes(&league.world);
        let Some(asset_index) = indexes.asset_index(&resolved_asset_id) else {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "world asset not found", "asset_id": resolved_asset_id })),
            )
                .into_response();
        };
        if league.world.world_assets[asset_index].owner_matrix_user_id != matrix_user_id {
            return (
                StatusCode::FORBIDDEN,
                Json(json!({ "error": "world asset belongs to another player", "asset_id": resolved_asset_id })),
            )
                .into_response();
        }
        let level_before = league.world.world_assets[asset_index].upgrade_level.max(1);
        let points_before = league.world.world_assets[asset_index].upgrade_points.max(0);
        let value_delta = if judgement.payout_status == "eligible" {
            (judgement.score / 4.0).round().max(1.0) as i64
        } else {
            0
        };
        let points_after = points_before.saturating_add(value_delta);
        let level_after = level_before
            .max(1)
            .saturating_add((points_after / 80).saturating_sub(points_before / 80));
        let upgrade_status = if judgement.payout_status == "eligible" {
            "applied".to_string()
        } else {
            "review_hold".to_string()
        };
        if value_delta > 0 {
            let asset = &mut league.world.world_assets[asset_index];
            asset.value_score = asset.value_score.saturating_add(value_delta);
            asset.upgrade_points = points_after;
            asset.upgrade_level = level_after.max(level_before);
            asset.last_upgrade_kind = Some("manual_upgrade".to_string());
            asset.status = "upgraded".to_string();
        }
        let upgrade = WorldAssetUpgrade {
            upgrade_id: league_hash_id(
                "world-asset-upgrade",
                &format!("{}:{}:{}", resolved_asset_id, now, body),
            ),
            asset_id: resolved_asset_id.clone(),
            matrix_user_id: matrix_user_id.clone(),
            body: body.clone(),
            upgrade_kind: "manual_upgrade".to_string(),
            score: judgement.score,
            grade: judgement.grade.clone(),
            judge_status: judgement.judge_status.clone(),
            status: upgrade_status,
            value_delta,
            level_before,
            level_after: level_after.max(level_before),
            created_at_epoch: now,
        };
        if judgement.payout_status == "eligible" {
            let mut player = ensure_league_player(&mut league, &matrix_user_id, None);
            player.xp = player.xp.saturating_add(judgement.score.round() as i64);
            player.reputation = player
                .reputation
                .saturating_add((judgement.score / 10.0).round() as i64);
            player.rating = player
                .rating
                .saturating_add(((judgement.score - 50.0) / 4.0).round() as i64);
            league
                .players_by_matrix_user
                .insert(matrix_user_id.clone(), player);
        }
        league.world.world_asset_upgrades.push(upgrade.clone());
        (
            league.clone(),
            league.world.world_assets[asset_index].clone(),
            upgrade,
        )
    };
    if let Err(response) =
        persist_league_state_after_command(&state, &snapshot.0, "world_asset_upgrade").await
    {
        return response;
    }
    let WorldRouteArtifacts {
        preview: route_preview,
        task_graph: route_task_graph,
        ..
    } = build_world_route_artifacts(&snapshot.0.world);
    (
        StatusCode::OK,
        Json(json!({
            "kind": "trillionnium_world_asset_upgrade",
            "world": "trillionnium_world",
            "asset": snapshot.1,
            "upgrade": snapshot.2,
            "route_preview": route_preview,
            "route_task_graph": route_task_graph,
        })),
    )
        .into_response()
}

pub(super) async fn upgrade_world_asset(
    Path(asset_id): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<WorldAssetUpgradeRequest>,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }
    upgrade_world_asset_inner(state, asset_id, payload).await
}

pub(super) async fn post_world_web_asset_upgrade(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(payload): Form<WorldWebAssetUpgradeRequest>,
) -> Response {
    let web_session = match authorize_league_web_session(&state, &headers, payload.csrf.as_deref())
    {
        Ok(value) => value,
        Err(response) => {
            if matches!(state.config().runtime_profile, RuntimeProfile::LocalDev)
                && cookie_value(&headers, &state.config().league_web_session_cookie_name).is_none()
            {
                None
            } else {
                return response;
            }
        }
    };
    let matrix_user_id = web_session
        .as_ref()
        .map(|session| session.matrix_user_id.clone())
        .or_else(|| {
            normalize_league_matrix_user(
                payload
                    .matrix_user_id
                    .as_deref()
                    .unwrap_or("@alice:local.dev"),
            )
        })
        .unwrap_or_else(|| "@alice:local.dev".to_string());
    let asset_id = payload
        .asset_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("latest")
        .to_string();
    let body = payload
        .body
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("Upgrade this world item for a customer deliverable: strengthen capability, evidence package, risk controls, next action loop, self-review, and side-quest handoff.")
        .to_string();
    let request = WorldAssetUpgradeRequest {
        matrix_user_id,
        room_id: web_session
            .as_ref()
            .and_then(|session| session.room_id.clone())
            .or_else(|| Some("!web-local:local.dev".to_string())),
        body,
    };
    let response = upgrade_world_asset_inner(state, asset_id, request).await;
    if response.status().is_success() {
        Redirect::to("/world?asset=upgraded").into_response()
    } else {
        response
    }
}

pub(super) async fn get_world_companies(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }
    let league = state.inner.league_state.lock().await;
    let WorldRouteArtifacts {
        preview: route_preview,
        task_graph: route_task_graph,
        ..
    } = build_world_route_artifacts(&league.world);
    (
        StatusCode::OK,
        Json(json!({
            "kind": "trillionnium_world_companies",
            "world": "trillionnium_world",
            "companies": league.world.world_companies,
            "route_preview": route_preview,
            "route_task_graph": route_task_graph,
        })),
    )
        .into_response()
}

pub(super) async fn create_world_company_inner(
    state: AppState,
    payload: WorldCompanyRequest,
) -> Response {
    let matrix_user_id = match normalize_league_matrix_user(&payload.matrix_user_id) {
        Some(value) => value,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "matrix_user_id is required" })),
            )
                .into_response()
        }
    };
    let body = match validate_text_payload(&payload.body, state.config().max_text_chars) {
        Ok(value) => value,
        Err(response) => return response,
    };
    let requested_asset_id = payload
        .asset_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("latest")
        .to_string();
    let judgement = judge_league_submission_with_pipeline(&state, &body, "world_company").await;
    let snapshot = {
        let now = Utc::now().timestamp();
        let mut league = state.inner.league_state.lock().await;
        let indexes = build_world_indexes(&league.world);
        let Some(asset) = indexes
            .resolve_asset_index(&requested_asset_id, &matrix_user_id)
            .and_then(|asset_index| league.world.world_assets.get(asset_index))
            .cloned()
        else {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "world asset not found", "asset_id": requested_asset_id })),
            )
                .into_response();
        };
        if asset.owner_matrix_user_id != matrix_user_id {
            return (
                StatusCode::FORBIDDEN,
                Json(json!({ "error": "world asset belongs to another player", "asset_id": asset.asset_id })),
            )
                .into_response();
        }
        let released = judgement.payout_status == "eligible";
        let revenue_score = if released {
            ((asset.value_score as f64) * 0.6 + judgement.score).round() as i64
        } else {
            0
        };
        let reputation_score = if released {
            ((asset.upgrade_level.max(1).saturating_mul(10)) as f64 + judgement.score / 2.0).round()
                as i64
        } else {
            0
        };
        let level = 1_i64.saturating_add((revenue_score / 100).max(0));
        let company_kind = if body.contains("店") || body.to_ascii_lowercase().contains("shop") {
            "shop"
        } else if body.contains("工坊") || body.to_ascii_lowercase().contains("studio") {
            "studio"
        } else {
            "company"
        };
        let company = WorldCompany {
            company_id: league_hash_id(
                "world-company",
                &format!("{}:{}:{}", matrix_user_id, asset.asset_id, now),
            ),
            owner_matrix_user_id: matrix_user_id.clone(),
            asset_id: asset.asset_id.clone(),
            location_id: asset.location_id.clone(),
            name: if company_kind == "shop" {
                "Mirror Market Shop".to_string()
            } else if company_kind == "studio" {
                "Trillionnium Craft Studio".to_string()
            } else {
                "Reality Venture Company".to_string()
            },
            company_kind: company_kind.to_string(),
            status: if released {
                "operating".to_string()
            } else {
                "review_hold".to_string()
            },
            revenue_score,
            reputation_score,
            level,
            created_at_epoch: now,
        };
        let shop = WorldShop {
            shop_id: league_hash_id(
                "world-shop",
                &format!("{}:{}:{}", matrix_user_id, company.company_id, now),
            ),
            company_id: company.company_id.clone(),
            owner_matrix_user_id: matrix_user_id.clone(),
            location_id: company.location_id.clone(),
            name: format!("{} Storefront", company.name),
            shop_kind: company_kind.to_string(),
            status: company.status.clone(),
            listing_count: if released { 1 } else { 0 },
            gross_merchandise_score: revenue_score.max(0),
            created_at_epoch: now,
        };
        let listing = WorldListing {
            listing_id: league_hash_id(
                "world-listing",
                &format!("{}:{}:{}", matrix_user_id, shop.shop_id, now),
            ),
            shop_id: shop.shop_id.clone(),
            company_id: company.company_id.clone(),
            owner_matrix_user_id: matrix_user_id.clone(),
            asset_id: asset.asset_id.clone(),
            title: body.chars().take(42).collect::<String>(),
            listing_kind: "service_offer".to_string(),
            status: company.status.clone(),
            price_credits: if released {
                (revenue_score / 2).max(10)
            } else {
                0
            },
            quality_score: if released {
                judgement.score.round() as i64
            } else {
                0
            },
            created_at_epoch: now,
        };
        let economy_event = if released {
            Some(WorldEconomyEvent {
                economy_event_id: league_hash_id(
                    "world-econ",
                    &format!("{}:{}:{}", matrix_user_id, listing.listing_id, now),
                ),
                matrix_user_id: matrix_user_id.clone(),
                event_kind: "company_launch".to_string(),
                subject_id: company.company_id.clone(),
                credits_delta: listing.price_credits,
                reputation_delta: reputation_score,
                created_at_epoch: now,
            })
        } else {
            None
        };
        if released {
            let mut player = ensure_league_player(&mut league, &matrix_user_id, None);
            player.xp = player.xp.saturating_add(judgement.score.round() as i64);
            player.reputation = player
                .reputation
                .saturating_add((judgement.score / 6.0).round() as i64);
            player.rating = player
                .rating
                .saturating_add(((judgement.score - 50.0) / 4.0).round() as i64);
            league.world.world_relationships.push(WorldRelationship {
                relationship_id: league_hash_id(
                    "world-rel",
                    &format!("{}:{}:{}", matrix_user_id, company.company_id, now),
                ),
                from_id: matrix_user_id.clone(),
                to_id: company.company_id.clone(),
                relation_kind: "owner".to_string(),
                strength: reputation_score,
                updated_at_epoch: now,
            });
            league
                .players_by_matrix_user
                .insert(matrix_user_id.clone(), player);
        }
        league.world.world_companies.push(company.clone());
        league.world.world_shops.push(shop.clone());
        league.world.world_listings.push(listing.clone());
        if let Some(economy_event) = economy_event.clone() {
            league.world.world_economy_events.push(economy_event);
        }
        (
            league.clone(),
            company,
            shop,
            listing,
            economy_event,
            judgement,
        )
    };
    if let Err(response) =
        persist_league_state_after_command(&state, &snapshot.0, "world_company").await
    {
        return response;
    }
    let WorldRouteArtifacts {
        preview: route_preview,
        task_graph: route_task_graph,
        ..
    } = build_world_route_artifacts(&snapshot.0.world);
    (
        StatusCode::OK,
        Json(json!({
            "kind": "trillionnium_world_company_created",
            "world": "trillionnium_world",
            "company": snapshot.1,
            "shop": snapshot.2,
            "listing": snapshot.3,
            "economy_event": snapshot.4,
            "judge_status": snapshot.5.judge_status,
            "payout_status": snapshot.5.payout_status,
            "route_preview": route_preview,
            "route_task_graph": route_task_graph,
        })),
    )
        .into_response()
}

pub(super) async fn create_world_company(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<WorldCompanyRequest>,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }
    create_world_company_inner(state, payload).await
}

pub(super) async fn post_world_web_company(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(payload): Form<WorldWebCompanyRequest>,
) -> Response {
    let web_session = match authorize_league_web_session(&state, &headers, payload.csrf.as_deref())
    {
        Ok(value) => value,
        Err(response) => {
            if matches!(state.config().runtime_profile, RuntimeProfile::LocalDev)
                && cookie_value(&headers, &state.config().league_web_session_cookie_name).is_none()
            {
                None
            } else {
                return response;
            }
        }
    };
    let matrix_user_id = web_session
        .as_ref()
        .map(|session| session.matrix_user_id.clone())
        .or_else(|| {
            normalize_league_matrix_user(
                payload
                    .matrix_user_id
                    .as_deref()
                    .unwrap_or("@alice:local.dev"),
            )
        })
        .unwrap_or_else(|| "@alice:local.dev".to_string());
    let body = payload
        .body
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("Launch a global-facing studio hub with this item: define customer deliverables, evidence package, risk controls, next action loop, self-review, and the first bounty route.")
        .to_string();
    let request = WorldCompanyRequest {
        matrix_user_id,
        asset_id: payload
            .asset_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string),
        body,
    };
    let response = create_world_company_inner(state, request).await;
    if response.status().is_success() {
        Redirect::to("/world?company=created").into_response()
    } else {
        response
    }
}

pub(super) async fn get_world_shops(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }
    let league = state.inner.league_state.lock().await;
    let WorldRouteArtifacts {
        preview: route_preview,
        task_graph: route_task_graph,
        ..
    } = build_world_route_artifacts(&league.world);
    (
        StatusCode::OK,
        Json(json!({
            "kind": "trillionnium_world_shops",
            "world": "trillionnium_world",
            "companies": league.world.world_companies,
            "shops": league.world.world_shops,
            "listings": league.world.world_listings,
            "economy_events": league.world.world_economy_events,
            "route_preview": route_preview,
            "route_task_graph": route_task_graph,
        })),
    )
        .into_response()
}

pub(super) async fn create_world_listing_inner(
    state: AppState,
    payload: WorldListingRequest,
) -> Response {
    let matrix_user_id = match normalize_league_matrix_user(&payload.matrix_user_id) {
        Some(value) => value,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "matrix_user_id is required" })),
            )
                .into_response()
        }
    };
    let body = match validate_text_payload(&payload.body, state.config().max_text_chars) {
        Ok(value) => value,
        Err(response) => return response,
    };
    let requested_company_id = payload
        .company_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("latest")
        .to_string();
    let judgement = judge_league_submission_with_pipeline(&state, &body, "world_listing").await;
    let snapshot = {
        let now = Utc::now().timestamp();
        let mut league = state.inner.league_state.lock().await;
        let indexes = build_world_indexes(&league.world);
        let Some(company_index) =
            indexes.resolve_company_index(&requested_company_id, &matrix_user_id)
        else {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "world company not found", "company_id": requested_company_id })),
            )
                .into_response();
        };
        let company_seed = league.world.world_companies[company_index].clone();
        if company_seed.owner_matrix_user_id != matrix_user_id {
            return (
                StatusCode::FORBIDDEN,
                Json(json!({ "error": "world company belongs to another player", "company_id": company_seed.company_id })),
            )
                .into_response();
        }
        if company_seed.status != "operating" {
            return (
                StatusCode::CONFLICT,
                Json(json!({
                    "error": "world company is not operating",
                    "company_id": company_seed.company_id,
                    "status": company_seed.status,
                })),
            )
                .into_response();
        }
        let shop_index = match indexes
            .shop_index_by_company_id
            .get(&company_seed.company_id)
            .copied()
        {
            Some(index) => index,
            None => {
                league.world.world_shops.push(WorldShop {
                    shop_id: league_hash_id(
                        "world-shop",
                        &format!("{}:{}:{}", matrix_user_id, company_seed.company_id, now),
                    ),
                    company_id: company_seed.company_id.clone(),
                    owner_matrix_user_id: matrix_user_id.clone(),
                    location_id: company_seed.location_id.clone(),
                    name: format!("{} Storefront", company_seed.name),
                    shop_kind: company_seed.company_kind.clone(),
                    status: company_seed.status.clone(),
                    listing_count: 0,
                    gross_merchandise_score: 0,
                    created_at_epoch: now,
                });
                league.world.world_shops.len() - 1
            }
        };
        let released = judgement.payout_status == "eligible";
        let quality_score = if released {
            judgement.score.round() as i64
        } else {
            0
        };
        let price_credits = if released {
            ((((company_seed.revenue_score.max(10) as f64) * 0.35) + judgement.score).round()
                as i64)
                .max(10)
        } else {
            0
        };
        let listing = WorldListing {
            listing_id: league_hash_id(
                "world-listing",
                &format!(
                    "{}:{}:{}",
                    matrix_user_id, league.world.world_shops[shop_index].shop_id, now
                ),
            ),
            shop_id: league.world.world_shops[shop_index].shop_id.clone(),
            company_id: company_seed.company_id.clone(),
            owner_matrix_user_id: matrix_user_id.clone(),
            asset_id: company_seed.asset_id.clone(),
            title: body.chars().take(48).collect::<String>(),
            listing_kind: if body.contains("订阅")
                || body.to_ascii_lowercase().contains("subscription")
            {
                "subscription_offer".to_string()
            } else {
                "service_offer".to_string()
            },
            status: if released {
                "listed".to_string()
            } else {
                "review_hold".to_string()
            },
            price_credits,
            quality_score,
            created_at_epoch: now,
        };
        let economy_event = if released {
            Some(WorldEconomyEvent {
                economy_event_id: league_hash_id(
                    "world-econ",
                    &format!("{}:{}:{}", matrix_user_id, listing.listing_id, now),
                ),
                matrix_user_id: matrix_user_id.clone(),
                event_kind: "listing_published".to_string(),
                subject_id: listing.listing_id.clone(),
                credits_delta: listing.price_credits,
                reputation_delta: (judgement.score / 5.0).round() as i64,
                created_at_epoch: now,
            })
        } else {
            None
        };
        if released {
            let reputation_delta = economy_event
                .as_ref()
                .map(|event| event.reputation_delta)
                .unwrap_or_default();
            league.world.world_shops[shop_index].listing_count = league.world.world_shops
                [shop_index]
                .listing_count
                .saturating_add(1);
            league.world.world_shops[shop_index].gross_merchandise_score = league.world.world_shops
                [shop_index]
                .gross_merchandise_score
                .saturating_add(listing.price_credits);
            league.world.world_companies[company_index].revenue_score =
                league.world.world_companies[company_index]
                    .revenue_score
                    .saturating_add(listing.price_credits);
            league.world.world_companies[company_index].reputation_score =
                league.world.world_companies[company_index]
                    .reputation_score
                    .saturating_add(reputation_delta);
            league.world.world_companies[company_index].level = 1_i64.saturating_add(
                (league.world.world_companies[company_index].revenue_score / 100).max(0),
            );
            let mut player = ensure_league_player(&mut league, &matrix_user_id, None);
            player.xp = player.xp.saturating_add(quality_score);
            player.reputation = player.reputation.saturating_add(reputation_delta);
            player.rating = player
                .rating
                .saturating_add(((judgement.score - 50.0) / 5.0).round() as i64);
            league
                .players_by_matrix_user
                .insert(matrix_user_id.clone(), player);
        }
        league.world.world_listings.push(listing.clone());
        if let Some(economy_event) = economy_event.clone() {
            league.world.world_economy_events.push(economy_event);
        }
        let company = league.world.world_companies[company_index].clone();
        let shop = league.world.world_shops[shop_index].clone();
        (
            league.clone(),
            company,
            shop,
            listing,
            economy_event,
            judgement,
        )
    };
    if let Err(response) =
        persist_league_state_after_command(&state, &snapshot.0, "world_listing").await
    {
        return response;
    }
    let WorldRouteArtifacts {
        preview: route_preview,
        task_graph: route_task_graph,
        ..
    } = build_world_route_artifacts(&snapshot.0.world);
    (
        StatusCode::OK,
        Json(json!({
            "kind": "trillionnium_world_listing_created",
            "world": "trillionnium_world",
            "company": snapshot.1,
            "shop": snapshot.2,
            "listing": snapshot.3,
            "economy_event": snapshot.4,
            "judge_status": snapshot.5.judge_status,
            "payout_status": snapshot.5.payout_status,
            "route_preview": route_preview,
            "route_task_graph": route_task_graph,
        })),
    )
        .into_response()
}

pub(super) async fn create_world_listing(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<WorldListingRequest>,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }
    create_world_listing_inner(state, payload).await
}

pub(super) async fn post_world_web_listing(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(payload): Form<WorldWebListingRequest>,
) -> Response {
    let web_session = match authorize_league_web_session(&state, &headers, payload.csrf.as_deref())
    {
        Ok(value) => value,
        Err(response) => {
            if matches!(state.config().runtime_profile, RuntimeProfile::LocalDev)
                && cookie_value(&headers, &state.config().league_web_session_cookie_name).is_none()
            {
                None
            } else {
                return response;
            }
        }
    };
    let matrix_user_id = web_session
        .as_ref()
        .map(|session| session.matrix_user_id.clone())
        .or_else(|| {
            normalize_league_matrix_user(
                payload
                    .matrix_user_id
                    .as_deref()
                    .unwrap_or("@alice:local.dev"),
            )
        })
        .unwrap_or_else(|| "@alice:local.dev".to_string());
    let body = payload
        .body
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("Publish a Trillionnium World service listing with deliverable, price logic, evidence package, customer promise, risk controls, self-review, and next action.")
        .to_string();
    let request = WorldListingRequest {
        matrix_user_id,
        company_id: payload
            .company_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string),
        body,
    };
    let response = create_world_listing_inner(state, request).await;
    if response.status().is_success() {
        Redirect::to("/world?listing=created").into_response()
    } else {
        response
    }
}

pub(super) async fn get_world_commerce(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }
    let league = state.inner.league_state.lock().await;
    let WorldRouteArtifacts {
        preview: route_preview,
        task_graph: route_task_graph,
        ..
    } = build_world_route_artifacts(&league.world);
    (
        StatusCode::OK,
        Json(json!({
            "kind": "trillionnium_world_commerce",
            "world": "trillionnium_world",
            "purchases": league.world.world_purchases,
            "work_orders": league.world.world_work_orders,
            "work_deliveries": league.world.world_work_deliveries,
            "work_acceptances": league.world.world_work_acceptances,
            "work_rejections": league.world.world_work_rejections,
            "work_reopens": league.world.world_work_reopens,
            "work_cancellations": league.world.world_work_cancellations,
            "economy_events": league.world.world_economy_events,
            "route_preview": route_preview,
            "route_task_graph": route_task_graph,
        })),
    )
        .into_response()
}

pub(super) async fn get_world_factions(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }
    let league = state.inner.league_state.lock().await;
    let world_indexes = build_world_indexes(&league.world);
    let factions: Vec<WorldFaction> = world_indexes
        .sorted_faction_ids
        .iter()
        .filter_map(|faction_id| league.world.world_factions.get(faction_id).cloned())
        .collect();
    (
        StatusCode::OK,
        Json(json!({
            "kind": "trillionnium_world_factions",
            "world": "trillionnium_world",
            "index_layer": "WorldIndexes::sorted_faction_ids_v1",
            "factions": factions,
            "standings": league.world.world_faction_standings,
        })),
    )
        .into_response()
}

pub(super) async fn settle_world_purchase_ledger_action(
    state: &AppState,
    room_id: Option<&str>,
    matrix_user_id: &str,
    purchase: &WorldPurchase,
    action: &str,
    intent_id: String,
    success_status: &str,
    idempotency_key: String,
    reference_id: String,
    message: &str,
    failure_context: &str,
    amount_override_credits: Option<i64>,
) -> LeagueLedgerSettlement {
    let ledger_amount_credits = amount_override_credits.unwrap_or_else(|| {
        if action == "grant" {
            world_seller_net_credits_for_price(purchase.price_credits)
        } else {
            purchase.price_credits
        }
    });
    let mut extra_ledger_body = Map::new();
    extra_ledger_body.insert("gross_amount".to_string(), json!(purchase.price_credits));
    extra_ledger_body.insert(
        "market_tax_amount".to_string(),
        json!(if action == "grant" {
            purchase.price_credits.saturating_sub(ledger_amount_credits)
        } else {
            0
        }),
    );
    CexTermExchangeBackend
        .execute_ledger_action(
            state,
            TermExchangeLedgerActionRequest {
                term_id: "world_commerce_purchase".to_string(),
                term_version: "v1".to_string(),
                domain: "trillionnium_world".to_string(),
                intent_id,
                intent_kind: match action {
                    "reserve" => term_exchange_protocol::EconomicIntentKind::Reserve,
                    "consume" => term_exchange_protocol::EconomicIntentKind::Consume,
                    "refund" => term_exchange_protocol::EconomicIntentKind::Refund,
                    "grant" => term_exchange_protocol::EconomicIntentKind::Settle,
                    _ => term_exchange_protocol::EconomicIntentKind::Settle,
                },
                room_id: room_id.map(ToString::to_string),
                matrix_user_id: matrix_user_id.to_string(),
                account_id_override: None,
                message: message.to_string(),
                failure_context: failure_context.to_string(),
                ledger_action: action.to_string(),
                success_status: success_status.to_string(),
                idempotency_key,
                idempotency_scope: format!("world_purchase_{action}"),
                reference_id: Some(reference_id),
                amount_credits: ledger_amount_credits,
                amount_validation_error: None,
                currency: "credits".to_string(),
                metadata: json!({
                    "purchase_id": purchase.purchase_id,
                    "listing_id": purchase.listing_id,
                    "buyer_matrix_user_id": purchase.buyer_matrix_user_id,
                    "seller_matrix_user_id": purchase.seller_matrix_user_id,
                    "price_credits": purchase.price_credits,
                    "ledger_action": action,
                    "zero_value_outcome": if action == "grant" && ledger_amount_credits <= 0 {
                        "zero_seller_net"
                    } else {
                        "value_bearing"
                    },
                }),
                extra_ledger_body,
            },
        )
        .await
        .into_legacy_settlement()
}

pub(super) async fn reserve_world_purchase_with_ledger(
    state: &AppState,
    payload: &WorldListingBuyRequest,
    purchase: &WorldPurchase,
) -> LeagueLedgerSettlement {
    settle_world_purchase_ledger_action(
        state,
        payload.room_id.as_deref(),
        &purchase.buyer_matrix_user_id,
        purchase,
        "reserve",
        format!("world_purchase:reserve:{}", purchase.purchase_id),
        "reserved",
        format!("world_purchase_reserve:{}", purchase.purchase_id),
        purchase.purchase_id.clone(),
        "world listing purchase reserve",
        "matrix identity could not be resolved for world purchase reserve",
        None,
    )
    .await
}

pub(super) async fn settle_world_purchase_with_ledger(
    state: &AppState,
    payload: &WorldListingBuyRequest,
    purchase: &WorldPurchase,
) -> LeagueLedgerSettlement {
    settle_world_purchase_ledger_action(
        state,
        payload.room_id.as_deref(),
        &purchase.seller_matrix_user_id,
        purchase,
        "grant",
        format!("world_purchase:grant:{}", purchase.purchase_id),
        "settled",
        format!("world_purchase:{}", purchase.purchase_id),
        purchase.listing_id.clone(),
        "world listing purchase settlement",
        "matrix identity could not be resolved for world purchase settlement",
        None,
    )
    .await
}

pub(super) async fn consume_world_purchase_with_ledger(
    state: &AppState,
    room_id: Option<&str>,
    purchase: &WorldPurchase,
) -> LeagueLedgerSettlement {
    settle_world_purchase_ledger_action(
        state,
        room_id,
        &purchase.buyer_matrix_user_id,
        purchase,
        "consume",
        format!("world_purchase:consume:{}", purchase.purchase_id),
        "consumed",
        format!("world_purchase_consume:{}", purchase.purchase_id),
        purchase.purchase_id.clone(),
        "world listing purchase consume",
        "matrix identity could not be resolved for world purchase consume",
        None,
    )
    .await
}

pub(super) async fn refund_world_purchase_with_ledger(
    state: &AppState,
    room_id: Option<&str>,
    purchase: &WorldPurchase,
    refund_scope: &str,
) -> LeagueLedgerSettlement {
    settle_world_purchase_ledger_action(
        state,
        room_id,
        &purchase.buyer_matrix_user_id,
        purchase,
        "refund",
        format!(
            "world_purchase:refund:{}:{}",
            purchase.purchase_id, refund_scope
        ),
        "refunded",
        format!(
            "world_purchase_refund:{}:{}",
            purchase.purchase_id, refund_scope
        ),
        purchase.purchase_id.clone(),
        "world listing purchase refund",
        "matrix identity could not be resolved for world purchase refund",
        None,
    )
    .await
}

pub(super) async fn chargeback_world_purchase_seller_with_ledger(
    state: &AppState,
    room_id: Option<&str>,
    purchase: &WorldPurchase,
    chargeback_scope: &str,
) -> LeagueLedgerSettlement {
    let (seller_settlement_active, typed_retrying_failed_chargeback) = {
        let league = state.inner.league_state.lock().await;
        (
            world_purchase_seller_settlement_active(&league.world, purchase),
            world_purchase_seller_chargeback_recoverable_hold(
                &league.world,
                purchase,
                chargeback_scope,
            ),
        )
    };
    // A compatibility `seller_chargeback_failed` status is not proof that the seller was ever
    // paid.  Retry is safe only when the immutable seller settlement receipt (or an exact,
    // recoverable-hold chargeback receipt) establishes the value-bearing amount.  This prevents a
    // synthetic/stale status from authorizing a new debit after restart.
    if !seller_settlement_active && !typed_retrying_failed_chargeback {
        return LeagueLedgerSettlement {
            status: "skipped_seller_not_settled".to_string(),
            account_id: purchase.ledger_account_id.clone(),
            error: Some("seller settlement is not active for chargeback".to_string()),
            ..Default::default()
        };
    }
    let seller_net_credits = world_seller_net_credits_for_price(purchase.price_credits);
    if seller_net_credits <= 0 {
        return LeagueLedgerSettlement {
            status: "skipped_zero_seller_net".to_string(),
            account_id: purchase.ledger_account_id.clone(),
            amount_credits: Some(0),
            ..Default::default()
        };
    }
    let reserve = settle_world_purchase_ledger_action(
        state,
        room_id,
        &purchase.seller_matrix_user_id,
        purchase,
        "reserve",
        format!(
            "world_purchase_seller_chargeback_reserve:{}:{}",
            purchase.purchase_id, chargeback_scope
        ),
        "seller_chargeback_reserved",
        format!(
            "world_purchase_seller_chargeback_reserve:{}:{}",
            purchase.purchase_id, chargeback_scope
        ),
        purchase.purchase_id.clone(),
        "world listing seller chargeback reserve",
        "matrix identity could not be resolved for world seller chargeback reserve",
        Some(seller_net_credits),
    )
    .await;
    let reserve_amount_matches = reserve.amount_credits == Some(seller_net_credits);
    if !reserve.progression_allowed_for_term(
        "world_commerce_purchase",
        &["seller_chargeback_reserved", "duplicate"],
    ) || !reserve_amount_matches
    {
        return LeagueLedgerSettlement {
            status: "seller_chargeback_reserve_failed".to_string(),
            account_id: reserve.account_id,
            entry_id: reserve.entry_id,
            balance_after: reserve.balance_after,
            error: reserve.error.or_else(|| {
                Some(if reserve_amount_matches {
                    format!("seller chargeback reserve did not complete: {}", reserve.status)
                } else {
                    format!(
                        "seller chargeback reserve amount mismatch: expected {seller_net_credits}, got {:?}",
                        reserve.amount_credits
                    )
                })
            }),
            amount_credits: None,
            term_exchange_receipt: reserve.term_exchange_receipt,
        };
    }
    let consume = settle_world_purchase_ledger_action(
        state,
        room_id,
        &purchase.seller_matrix_user_id,
        purchase,
        "consume",
        format!(
            "world_purchase_seller_chargeback_consume:{}:{}",
            purchase.purchase_id, chargeback_scope
        ),
        "seller_chargeback_consumed",
        format!(
            "world_purchase_seller_chargeback_consume:{}:{}",
            purchase.purchase_id, chargeback_scope
        ),
        purchase.purchase_id.clone(),
        "world listing seller chargeback consume",
        "matrix identity could not be resolved for world seller chargeback consume",
        Some(seller_net_credits),
    )
    .await;
    let consume_amount_matches = consume.amount_credits == Some(seller_net_credits);
    if !consume.progression_allowed_for_term(
        "world_commerce_purchase",
        &["seller_chargeback_consumed", "duplicate"],
    ) || !consume_amount_matches
    {
        return LeagueLedgerSettlement {
            status: "seller_chargeback_failed".to_string(),
            account_id: consume.account_id,
            entry_id: consume.entry_id,
            balance_after: consume.balance_after,
            error: consume.error.or_else(|| {
                Some(if consume_amount_matches {
                    format!("seller chargeback consume did not complete: {}", consume.status)
                } else {
                    format!(
                        "seller chargeback consume amount mismatch: expected {seller_net_credits}, got {:?}",
                        consume.amount_credits
                    )
                })
            }),
            amount_credits: None,
            term_exchange_receipt: consume.term_exchange_receipt,
        };
    }
    consume
}

pub(super) async fn reserve_reopened_world_purchase_with_ledger(
    state: &AppState,
    room_id: Option<&str>,
    purchase: &WorldPurchase,
    reopen: &WorldWorkReopen,
) -> LeagueLedgerSettlement {
    settle_world_purchase_ledger_action(
        state,
        room_id,
        &purchase.buyer_matrix_user_id,
        purchase,
        "reserve",
        format!(
            "world_purchase_reopen_reserve:{}:{}",
            purchase.purchase_id, reopen.reopen_id
        ),
        "reserved",
        format!(
            "world_purchase_reopen_reserve:{}:{}",
            purchase.purchase_id, reopen.reopen_id
        ),
        purchase.purchase_id.clone(),
        "world listing purchase reopen reserve",
        "matrix identity could not be resolved for world purchase reopen reserve",
        None,
    )
    .await
}

pub(super) async fn settle_reopened_world_purchase_with_ledger(
    state: &AppState,
    room_id: Option<&str>,
    purchase: &WorldPurchase,
    reopen: &WorldWorkReopen,
) -> LeagueLedgerSettlement {
    settle_world_purchase_ledger_action(
        state,
        room_id,
        &purchase.seller_matrix_user_id,
        purchase,
        "grant",
        format!(
            "world_purchase_reopen_settlement:{}:{}",
            purchase.purchase_id, reopen.reopen_id
        ),
        "reopened_settled",
        format!(
            "world_purchase_reopen_settlement:{}:{}",
            purchase.purchase_id, reopen.reopen_id
        ),
        purchase.listing_id.clone(),
        "world listing purchase reopen settlement",
        "matrix identity could not be resolved for world purchase reopen settlement",
        None,
    )
    .await
}

pub(super) async fn buy_world_listing_inner(
    state: AppState,
    listing_id: String,
    payload: WorldListingBuyRequest,
) -> Response {
    let buyer_matrix_user_id = match normalize_league_matrix_user(&payload.matrix_user_id) {
        Some(value) => value,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "matrix_user_id is required" })),
            )
                .into_response()
        }
    };
    let brief = match validate_text_payload(
        payload
            .body
            .as_deref()
            .unwrap_or("Accept this quest card and open an adventure commission: confirm customer deliverables, evidence package, rating standards, risk controls, next action, and self-review."),
        state.config().max_text_chars,
    ) {
        Ok(value) => value,
        Err(response) => return response,
    };
    let snapshot = {
        let now = Utc::now().timestamp();
        let mut league = state.inner.league_state.lock().await;
        let indexes = build_world_indexes(&league.world);
        let Some(listing_index) = indexes.resolve_buyable_listing_index(
            &league.world,
            &listing_id,
            &buyer_matrix_user_id,
        ) else {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "world listing not found", "listing_id": listing_id })),
            )
                .into_response();
        };
        let listing = league.world.world_listings[listing_index].clone();
        if listing.status != "listed" {
            return (
                StatusCode::CONFLICT,
                Json(json!({
                    "error": "world listing is not open for purchase",
                    "listing_id": listing.listing_id,
                    "status": listing.status,
                })),
            )
                .into_response();
        }
        let company_index = indexes.company_index(&listing.company_id);
        let shop_index = indexes.shop_index(&listing.shop_id);
        let location_id = indexes
            .company_location_id(&listing.company_id)
            .or_else(|| indexes.shop_location_id(&listing.shop_id))
            .unwrap_or("zbj-market-gate");
        let faction_id = world_faction_for_location(location_id);
        let market_simulation = world_market_simulation_json(&league.world, &listing, now);
        let price_credits = market_simulation
            .get("dynamic_price_credits")
            .and_then(Value::as_i64)
            .unwrap_or_else(|| listing.price_credits.max(1));
        let reputation_delta = (listing.quality_score / 5).max(1);
        // Entity identity is an operation identity, not a timestamp.  Use the number of prior
        // *same request* attempts as the deterministic sequence so a lost response can replay
        // the same Ledger intent, while a later intentional purchase (after a terminal prior
        // order) still receives a fresh identity.
        let purchase_attempt = league
            .world
            .world_work_orders
            .iter()
            .filter(|work_order| {
                work_order.listing_id == listing.listing_id
                    && work_order.buyer_matrix_user_id == buyer_matrix_user_id
                    && work_order.brief == brief
            })
            .count();
        let existing_pending = league
            .world
            .world_work_orders
            .iter()
            .rev()
            .find(|work_order| {
                work_order.listing_id == listing.listing_id
                    && work_order.buyer_matrix_user_id == buyer_matrix_user_id
                    && work_order.brief == brief
                    && !matches!(
                        work_order.status.as_str(),
                        "completed" | "rejected_refunded" | "cancelled_refunded"
                    )
            })
            .and_then(|work_order| {
                league
                    .world
                    .world_purchases
                    .iter()
                    .find(|purchase| purchase.purchase_id == work_order.purchase_id)
                    .cloned()
                    .map(|purchase| (purchase, work_order.clone()))
            });
        let (purchase, work_order) =
            if let Some((existing_purchase, existing_work_order)) = existing_pending {
                // Reuse the persisted operation and its scoped Ledger identities on a retry.  The
                // state machine below will reconcile any pending/failed remote outcome.
                (existing_purchase, existing_work_order)
            } else {
                let purchase_id = league_hash_id(
                    "world-purchase",
                    &format!(
                        "{}:{}:{}:{}",
                        buyer_matrix_user_id, listing.listing_id, purchase_attempt, brief
                    ),
                );
                let purchase = WorldPurchase {
                    purchase_id: purchase_id.clone(),
                    listing_id: listing.listing_id.clone(),
                    shop_id: listing.shop_id.clone(),
                    company_id: listing.company_id.clone(),
                    buyer_matrix_user_id: buyer_matrix_user_id.clone(),
                    seller_matrix_user_id: listing.owner_matrix_user_id.clone(),
                    price_credits,
                    status: "pending_payment".to_string(),
                    ledger_status: Some("pending".to_string()),
                    ledger_account_id: None,
                    ledger_entry_id: None,
                    ledger_balance_after: None,
                    ledger_error: None,
                    buyer_ledger_status: Some("pending".to_string()),
                    buyer_ledger_account_id: None,
                    buyer_ledger_entry_id: None,
                    buyer_ledger_balance_after: None,
                    buyer_ledger_error: None,
                    buyer_consume_status: Some("pending_acceptance".to_string()),
                    buyer_consume_entry_id: None,
                    buyer_consume_balance_after: None,
                    buyer_consume_error: None,
                    created_at_epoch: now,
                };
                let work_order = WorldWorkOrder {
                    work_order_id: league_hash_id("world-work", &purchase_id),
                    purchase_id: purchase.purchase_id.clone(),
                    listing_id: listing.listing_id.clone(),
                    buyer_matrix_user_id: buyer_matrix_user_id.clone(),
                    seller_matrix_user_id: listing.owner_matrix_user_id.clone(),
                    company_id: listing.company_id.clone(),
                    status: "open".to_string(),
                    brief: brief.clone(),
                    value_score: price_credits.saturating_add(listing.quality_score.max(0)),
                    created_at_epoch: now,
                };
                league.world.world_purchases.push(purchase.clone());
                league.world.world_work_orders.push(work_order.clone());
                (purchase, work_order)
            };
        let company = company_index.map(|index| league.world.world_companies[index].clone());
        let shop = shop_index.map(|index| league.world.world_shops[index].clone());
        (
            league.clone(),
            purchase,
            work_order,
            listing,
            company,
            shop,
            market_simulation,
            reputation_delta,
            faction_id.to_string(),
        )
    };
    // Commit the operation identity before crossing the Ledger boundary.  If the process dies
    // after reserve/settlement but before the response write, the next request can find this
    // deterministic pending work order and replay the same scoped intents instead of minting a
    // second purchase.
    if let Err(response) =
        // The pending purchase/work-order identity is durable before the first remote Ledger
        // request.  Reuse the canonical command write-set so normalized final cutover persists
        // this pre-network snapshot through the same typed SQL helpers as the final projection.
        persist_league_state_after_command(&state, &snapshot.0, "world_buy").await
    {
        return response;
    }
    let buyer_reserve = reserve_world_purchase_with_ledger(&state, &payload, &snapshot.1).await;
    let expected_buyer_amount = world_purchase_buyer_amount(&snapshot.1);
    let buyer_reserve_progression_allowed = buyer_reserve
        .progression_allowed_for_term("world_commerce_purchase", &["reserved", "duplicate"]);
    let buyer_reserve_amount_matches = expected_buyer_amount
        .is_some_and(|expected| buyer_reserve.amount_credits == Some(expected));
    let buyer_reserved = buyer_reserve_progression_allowed && buyer_reserve_amount_matches;
    let settlement = if buyer_reserved {
        settle_world_purchase_with_ledger(&state, &payload, &snapshot.1).await
    } else {
        LeagueLedgerSettlement {
            status: "skipped_buyer_reserve".to_string(),
            error: Some(
                "seller settlement skipped because buyer reserve did not complete".to_string(),
            ),
            ..Default::default()
        }
    };
    let buyer_reserve_receipt = buyer_reserve.term_exchange_receipt.clone();
    let seller_settlement_receipt = settlement.term_exchange_receipt.clone();
    let final_snapshot = {
        let mut league = state.inner.league_state.lock().await;
        record_world_term_exchange_receipt(&mut league.world, buyer_reserve_receipt);
        record_world_term_exchange_receipt(&mut league.world, seller_settlement_receipt);
        let indexes = build_world_indexes(&league.world);
        let mut purchase = snapshot.1.clone();
        let mut work_order = snapshot.2.clone();
        let mut company = snapshot.4.clone();
        let mut shop = snapshot.5.clone();
        let mut economy_event = None;
        let mut seller_standing = None;
        let mut buyer_standing = None;
        let expected_seller_net_credits =
            world_seller_net_credits_for_price(purchase.price_credits);
        let settlement_progression_allowed = settlement
            .progression_allowed_for_term("world_commerce_purchase", &["settled", "duplicate"]);
        let settlement_amount_matches =
            settlement.amount_credits == Some(expected_seller_net_credits);
        let seller_zero_net_skipped =
            world_settlement_is_zero_seller_net_skip(&settlement, expected_seller_net_credits);
        let remote_released = (settlement_progression_allowed && settlement_amount_matches)
            || seller_zero_net_skipped;
        let self_dealing_purchase = purchase.buyer_matrix_user_id == purchase.seller_matrix_user_id;
        // The economy event is the durable projection key.  Once it exists, a retry must not
        // apply the commercial projection a second time (or regress a terminal purchase when a
        // later Ledger lookup is temporarily unavailable).
        let purchase_event_id = league_hash_id(
            "world-econ",
            &format!(
                "{}:{}:{}",
                purchase.buyer_matrix_user_id, purchase.purchase_id, purchase.created_at_epoch
            ),
        );
        let market_tax_event_id = league_hash_id(
            "world-market-tax",
            &format!(
                "{}:{}:{}",
                purchase.buyer_matrix_user_id, purchase.purchase_id, purchase.created_at_epoch
            ),
        );
        let purchase_projection_marker = world_purchase_projection_marker(
            &league.world,
            &purchase,
            &purchase_event_id,
            &purchase.seller_matrix_user_id,
            "listing_purchase",
            expected_seller_net_credits,
            snapshot.7,
        );
        let tax_projection_marker = world_purchase_projection_marker(
            &league.world,
            &purchase,
            &market_tax_event_id,
            &purchase.buyer_matrix_user_id,
            "market_tax_sink",
            world_market_tax_credits_for_price(purchase.price_credits).saturating_neg(),
            0,
        );
        let purchase_projection_already_applied = purchase_projection_marker == Some(true);
        let tax_projection_already_applied = tax_projection_marker == Some(true);
        let projection_already_applied = if self_dealing_purchase {
            tax_projection_already_applied
        } else {
            purchase_projection_already_applied
        };
        let projection_event_present = if self_dealing_purchase {
            tax_projection_marker.is_some()
        } else {
            purchase_projection_marker.is_some()
        };
        // Either deterministic marker being present without its exact tuple/receipt evidence is
        // a collision.  Do not let a valid primary marker hide a poisoned tax marker (or vice
        // versa) and then continue a value-bearing retry.
        let projection_event_poisoned =
            purchase_projection_marker == Some(false) || tax_projection_marker == Some(false);
        // A legacy marker can never authorize a release by itself.  If its deterministic id is
        // present but the tuple/receipt pair is invalid, fail closed and keep the operation
        // retryable rather than applying a second commercial projection.
        let released =
            !projection_event_poisoned && (remote_released || projection_already_applied);

        // Do not overwrite an already-projected operation with an ambiguous/failed retry result.
        // A successful duplicate response may still refresh the diagnostic fields.
        if !projection_already_applied || remote_released {
            purchase.buyer_ledger_status = Some(buyer_reserve.status.clone());
            purchase.buyer_ledger_account_id = buyer_reserve.account_id.clone();
            purchase.buyer_ledger_entry_id = buyer_reserve.entry_id.clone();
            purchase.buyer_ledger_balance_after = buyer_reserve.balance_after;
            purchase.buyer_ledger_error = buyer_reserve.error.clone().or_else(|| {
                (buyer_reserve_progression_allowed && !buyer_reserve_amount_matches).then(|| {
                    format!(
                        "buyer reserve amount mismatch: expected {:?}, got {:?}",
                        expected_buyer_amount, buyer_reserve.amount_credits
                    )
                })
            });
            purchase.ledger_status = Some(settlement.status.clone());
            purchase.ledger_account_id = settlement.account_id.clone();
            purchase.ledger_entry_id = settlement.entry_id.clone();
            purchase.ledger_balance_after = settlement.balance_after;
            purchase.ledger_error = settlement.error.clone().or_else(|| {
                (settlement_progression_allowed && !settlement_amount_matches).then(|| {
                    format!(
                        "seller settlement amount mismatch: expected {expected_seller_net_credits}, got {:?}",
                        settlement.amount_credits
                    )
                })
            });
        }
        purchase.status = if buyer_reserved || projection_already_applied {
            if released {
                "reserved".to_string()
            } else if settlement.status.starts_with("skipped") {
                "seller_settlement_pending".to_string()
            } else {
                "seller_settlement_failed".to_string()
            }
        } else if buyer_reserve.status.starts_with("skipped") {
            "payment_hold".to_string()
        } else {
            "buyer_reserve_failed".to_string()
        };
        work_order.status = if released {
            "open".to_string()
        } else if buyer_reserved {
            purchase.status.clone()
        } else {
            "payment_hold".to_string()
        };
        indexes.replace_purchase_by_id(&mut league.world, &purchase);
        indexes.replace_work_order_by_id(&mut league.world, &work_order);
        if released
            && !self_dealing_purchase
            && !purchase_projection_already_applied
            && !projection_event_present
        {
            if let Some(index) = indexes.shop_index(&purchase.shop_id) {
                league.world.world_shops[index].gross_merchandise_score = league.world.world_shops
                    [index]
                    .gross_merchandise_score
                    .saturating_add(purchase.price_credits);
            }
            if let Some(index) = indexes.company_index(&purchase.company_id) {
                league.world.world_companies[index].revenue_score = league.world.world_companies
                    [index]
                    .revenue_score
                    .saturating_add(purchase.price_credits);
                league.world.world_companies[index].reputation_score = league.world.world_companies
                    [index]
                    .reputation_score
                    .saturating_add(snapshot.7);
                league.world.world_companies[index].level = 1_i64.saturating_add(
                    (league.world.world_companies[index].revenue_score / 100).max(0),
                );
            }
            let mut buyer = ensure_league_player(&mut league, &purchase.buyer_matrix_user_id, None);
            buyer.xp = buyer
                .xp
                .saturating_add((snapshot.3.quality_score / 10).max(1));
            buyer.reputation = buyer.reputation.saturating_add(1);
            buyer.rating = buyer.rating.saturating_add(1);
            league
                .players_by_matrix_user
                .insert(purchase.buyer_matrix_user_id.clone(), buyer);
            let mut seller =
                ensure_league_player(&mut league, &purchase.seller_matrix_user_id, None);
            seller.xp = seller
                .xp
                .saturating_add((snapshot.3.quality_score / 2).max(1));
            seller.reputation = seller.reputation.saturating_add(snapshot.7);
            seller.rating = seller
                .rating
                .saturating_add((snapshot.3.quality_score / 10).max(1));
            if settlement
                .amount_credits
                .and_then(exact_credits_to_legacy_display)
                .is_some()
            {
                // Compatibility projection only: seller value is authorized by the exact
                // settlement receipt, never by the legacy purchase price field.
                if let Some(next) = checked_legacy_display_add(
                    seller.earned_credits,
                    settlement.amount_credits.unwrap_or_default(),
                ) {
                    seller.earned_credits = next;
                }
            }
            league
                .players_by_matrix_user
                .insert(purchase.seller_matrix_user_id.clone(), seller);
            let purchase_event = WorldEconomyEvent {
                economy_event_id: purchase_event_id.clone(),
                matrix_user_id: purchase.seller_matrix_user_id.clone(),
                event_kind: "listing_purchase".to_string(),
                subject_id: purchase.purchase_id.clone(),
                credits_delta: settlement
                    .amount_credits
                    .expect("released seller settlement must carry exact amount_credits"),
                reputation_delta: snapshot.7,
                created_at_epoch: purchase.created_at_epoch,
            };
            let market_tax_event = WorldEconomyEvent {
                economy_event_id: market_tax_event_id.clone(),
                matrix_user_id: purchase.buyer_matrix_user_id.clone(),
                event_kind: "market_tax_sink".to_string(),
                subject_id: purchase.purchase_id.clone(),
                credits_delta: snapshot
                    .6
                    .get("market_tax_credits")
                    .and_then(Value::as_i64)
                    .unwrap_or(1)
                    .saturating_neg(),
                reputation_delta: 0,
                created_at_epoch: purchase.created_at_epoch,
            };
            league.world.world_relationships.push(WorldRelationship {
                relationship_id: league_hash_id(
                    "world-rel",
                    &format!(
                        "{}:{}:{}",
                        purchase.buyer_matrix_user_id,
                        purchase.company_id,
                        purchase.created_at_epoch
                    ),
                ),
                from_id: purchase.buyer_matrix_user_id.clone(),
                to_id: purchase.company_id.clone(),
                relation_kind: "customer".to_string(),
                strength: snapshot.7,
                updated_at_epoch: purchase.created_at_epoch,
            });
            seller_standing = Some(upsert_world_faction_standing(
                &mut league,
                &purchase.seller_matrix_user_id,
                &snapshot.8,
                snapshot.7,
                purchase.created_at_epoch,
            ));
            buyer_standing = Some(upsert_world_faction_standing(
                &mut league,
                &purchase.buyer_matrix_user_id,
                &snapshot.8,
                1,
                purchase.created_at_epoch,
            ));
            push_world_economy_event_once(&mut league.world, purchase_event.clone());
            push_world_economy_event_once(&mut league.world, market_tax_event);
            company = indexes
                .company_index(&purchase.company_id)
                .map(|index| league.world.world_companies[index].clone());
            shop = indexes
                .shop_index(&purchase.shop_id)
                .map(|index| league.world.world_shops[index].clone());
            economy_event = Some(purchase_event);
        } else if released && !self_dealing_purchase && purchase_projection_already_applied {
            economy_event = league
                .world
                .world_economy_events
                .iter()
                .find(|event| event.economy_event_id == purchase_event_id)
                .cloned();
            seller_standing = indexes
                .faction_standing_index(&purchase.seller_matrix_user_id, &snapshot.8)
                .and_then(|index| league.world.world_faction_standings.get(index).cloned());
            buyer_standing = indexes
                .faction_standing_index(&purchase.buyer_matrix_user_id, &snapshot.8)
                .and_then(|index| league.world.world_faction_standings.get(index).cloned());
            company = indexes
                .company_index(&purchase.company_id)
                .and_then(|index| league.world.world_companies.get(index).cloned());
            shop = indexes
                .shop_index(&purchase.shop_id)
                .and_then(|index| league.world.world_shops.get(index).cloned());
            // A crash can persist the primary purchase projection before its tax-sink marker.
            // Repair the missing compatibility sink exactly once, but never overwrite a
            // present/poisoned marker.
            if tax_projection_marker.is_none() {
                let market_tax_event = WorldEconomyEvent {
                    economy_event_id: market_tax_event_id,
                    matrix_user_id: purchase.buyer_matrix_user_id.clone(),
                    event_kind: "market_tax_sink".to_string(),
                    subject_id: purchase.purchase_id.clone(),
                    credits_delta: world_market_tax_credits_for_price(purchase.price_credits)
                        .saturating_neg(),
                    reputation_delta: 0,
                    created_at_epoch: purchase.created_at_epoch,
                };
                push_world_economy_event_once(&mut league.world, market_tax_event);
            }
        } else if released
            && self_dealing_purchase
            && !tax_projection_already_applied
            && !projection_event_present
        {
            let market_tax_event = WorldEconomyEvent {
                economy_event_id: market_tax_event_id,
                matrix_user_id: purchase.buyer_matrix_user_id.clone(),
                event_kind: "market_tax_sink".to_string(),
                subject_id: purchase.purchase_id.clone(),
                credits_delta: snapshot
                    .6
                    .get("market_tax_credits")
                    .and_then(Value::as_i64)
                    .unwrap_or(1)
                    .saturating_neg(),
                reputation_delta: 0,
                created_at_epoch: purchase.created_at_epoch,
            };
            push_world_economy_event_once(&mut league.world, market_tax_event);
        }
        (
            league.clone(),
            purchase,
            work_order,
            company,
            shop,
            economy_event,
            seller_standing,
            buyer_standing,
        )
    };
    if let Err(response) =
        persist_league_state_after_command(&state, &final_snapshot.0, "world_buy").await
    {
        return response;
    }
    let WorldRouteArtifacts {
        preview: route_preview,
        task_graph: route_task_graph,
        ..
    } = build_world_route_artifacts(&final_snapshot.0.world);
    (
        StatusCode::OK,
        Json(json!({
            "kind": "trillionnium_world_listing_purchase",
            "world": "trillionnium_world",
            "purchase": final_snapshot.1,
            "work_order": final_snapshot.2,
            "listing": snapshot.3,
            "company": final_snapshot.3,
            "shop": final_snapshot.4,
            "economy_event": final_snapshot.5,
            "seller_standing": final_snapshot.6,
            "buyer_standing": final_snapshot.7,
            "market_simulation": snapshot.6,
            "buyer_ledger_status": buyer_reserve.status,
            "buyer_ledger_entry_id": buyer_reserve.entry_id,
            "buyer_ledger_error": buyer_reserve.error,
            "ledger_status": settlement.status,
            "ledger_entry_id": settlement.entry_id,
            "ledger_error": settlement.error,
            "route_preview": route_preview,
            "route_task_graph": route_task_graph,
        })),
    )
        .into_response()
}

pub(super) async fn buy_world_listing(
    Path(listing_id): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<WorldListingBuyRequest>,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }
    buy_world_listing_inner(state, listing_id, payload).await
}

pub(super) async fn post_world_web_listing_buy(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(payload): Form<WorldWebListingBuyRequest>,
) -> Response {
    let web_session = match authorize_league_web_session(&state, &headers, payload.csrf.as_deref())
    {
        Ok(value) => value,
        Err(response) => {
            if matches!(state.config().runtime_profile, RuntimeProfile::LocalDev)
                && cookie_value(&headers, &state.config().league_web_session_cookie_name).is_none()
            {
                None
            } else {
                return response;
            }
        }
    };
    let matrix_user_id = web_session
        .as_ref()
        .map(|session| session.matrix_user_id.clone())
        .or_else(|| {
            normalize_league_matrix_user(
                payload
                    .matrix_user_id
                    .as_deref()
                    .unwrap_or("@alice:local.dev"),
            )
        })
        .unwrap_or_else(|| "@alice:local.dev".to_string());
    let listing_id = payload
        .listing_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("latest")
        .to_string();
    let request = WorldListingBuyRequest {
        matrix_user_id,
        room_id: web_session
            .as_ref()
            .and_then(|session| session.room_id.clone())
            .or_else(|| Some("!web-local:local.dev".to_string())),
        body: payload
            .body
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string),
    };
    let response = buy_world_listing_inner(state, listing_id, request).await;
    if response.status().is_success() {
        Redirect::to("/world?purchase=created").into_response()
    } else {
        response
    }
}

pub(super) async fn deliver_world_work_order_inner(
    state: AppState,
    work_order_id: String,
    payload: WorldWorkDeliverRequest,
) -> Response {
    let matrix_user_id = match normalize_league_matrix_user(&payload.matrix_user_id) {
        Some(value) => value,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "matrix_user_id is required" })),
            )
                .into_response()
        }
    };
    let body = match validate_text_payload(&payload.body, state.config().max_text_chars) {
        Ok(value) => value,
        Err(response) => return response,
    };
    let _room_id = payload.room_id.as_deref();
    let judgement =
        judge_league_submission_with_pipeline(&state, &body, "world_work_delivery").await;
    let snapshot = {
        let now = Utc::now().timestamp();
        let mut league = state.inner.league_state.lock().await;
        let indexes = build_world_indexes(&league.world);
        let Some(work_index) =
            indexes.resolve_deliverable_work_order_index(&work_order_id, &matrix_user_id)
        else {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "world work order not found", "work_order_id": work_order_id })),
            )
                .into_response();
        };
        if league.world.world_work_orders[work_index].seller_matrix_user_id != matrix_user_id {
            return (
                StatusCode::FORBIDDEN,
                Json(json!({ "error": "work order belongs to another seller", "work_order_id": work_order_id })),
            )
                .into_response();
        }
        let work_order_status = league.world.world_work_orders[work_index].status.clone();
        let reopen_attempt = world_reopen_attempt(
            &league.world,
            &league.world.world_work_orders[work_index].work_order_id,
        );
        let deterministic_delivery_id = world_work_delivery_id(
            &league.world.world_work_orders[work_index].work_order_id,
            &matrix_user_id,
            reopen_attempt,
            &body,
        );
        if let Some(existing) = league
            .world
            .world_work_deliveries
            .iter()
            .find(|delivery| delivery.delivery_id == deterministic_delivery_id)
        {
            if !world_work_delivery_tuple_matches(
                existing,
                &league.world.world_work_orders[work_index].work_order_id,
                &matrix_user_id,
                &body,
            ) {
                return (
                    StatusCode::CONFLICT,
                    Json(json!({
                        "error": "world work delivery identity collision",
                        "delivery_id": deterministic_delivery_id,
                        "work_order_id": work_order_id,
                    })),
                )
                    .into_response();
            }
        }
        let existing_delivery = world_work_delivery_for_request(
            &league.world,
            &league.world.world_work_orders[work_index].work_order_id,
            &matrix_user_id,
            reopen_attempt,
            &body,
            &work_order_status,
        )
        .cloned();
        if !matches!(work_order_status.as_str(), "open" | "delivery_review_hold") {
            // An exact terminal delivery is a safe response-loss replay.  Other terminal order
            // states remain conflicts and cannot be adopted by a new request.
            if existing_delivery
                .as_ref()
                .is_none_or(|delivery| delivery.status != "delivered")
            {
                return (
                StatusCode::CONFLICT,
                Json(json!({ "error": "work order is not deliverable", "status": work_order_status, "work_order_id": work_order_id })),
            )
                .into_response();
            }
        }
        let work_order_seed = league.world.world_work_orders[work_index].clone();
        let Some(purchase_index) = indexes
            .purchase_index_by_id
            .get(&work_order_seed.purchase_id)
            .copied()
        else {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "world purchase not found for work order", "work_order_id": work_order_id, "purchase_id": work_order_seed.purchase_id })),
            )
                .into_response();
        };
        let purchase_seed = league.world.world_purchases[purchase_index].clone();
        if !world_purchase_seller_settlement_active(&league.world, &purchase_seed) {
            return (
                StatusCode::CONFLICT,
                Json(json!({
                    "error": "world purchase seller settlement is not active",
                    "work_order_id": work_order_id,
                    "purchase_id": purchase_seed.purchase_id,
                    "purchase_status": purchase_seed.status,
                    "ledger_status": purchase_seed.ledger_status,
                })),
            )
                .into_response();
        }
        let faction_id = "faction-market-guild";
        let requested_delivery_status = if judgement.payout_status == "eligible" {
            "delivered"
        } else {
            "review_hold"
        };
        // A terminal delivery is immutable.  A response-loss retry may re-run the judge, but a
        // late review-hold result must not downgrade the already delivered row or its work order.
        let delivery_status = if existing_delivery
            .as_ref()
            .is_some_and(|delivery| delivery.status == "delivered")
        {
            "delivered"
        } else {
            requested_delivery_status
        };
        let mut delivery = WorldWorkDelivery {
            delivery_id: existing_delivery
                .as_ref()
                .map(|delivery| delivery.delivery_id.clone())
                .unwrap_or(deterministic_delivery_id),
            work_order_id: work_order_seed.work_order_id.clone(),
            matrix_user_id: matrix_user_id.clone(),
            body: body.clone(),
            score: judgement.score,
            judge_status: judgement.judge_status.clone(),
            status: delivery_status.to_string(),
            created_at_epoch: existing_delivery
                .as_ref()
                .map(|delivery| delivery.created_at_epoch)
                .unwrap_or(now),
        };
        if let Some(existing) = league
            .world
            .world_work_deliveries
            .iter()
            .find(|existing| existing.delivery_id == delivery.delivery_id)
        {
            if existing.status == "delivered" {
                delivery = existing.clone();
            }
        }
        league.world.world_work_orders[work_index].status = if delivery_status == "delivered" {
            "delivered".to_string()
        } else {
            "delivery_review_hold".to_string()
        };
        let self_dealing_work =
            work_order_seed.buyer_matrix_user_id == work_order_seed.seller_matrix_user_id;
        let reputation_delta = if delivery.status == "delivered" && !self_dealing_work {
            (delivery.score / 10.0).round() as i64
        } else {
            0
        };
        let delivery_event_id = world_work_delivery_event_id(&delivery);
        let legacy_delivery_event_id = legacy_world_work_delivery_event_id(&delivery);
        let existing_economy_event = league
            .world
            .world_economy_events
            .iter()
            .find(|event| {
                (event.economy_event_id == delivery_event_id
                    || event.economy_event_id == legacy_delivery_event_id)
                    && world_economy_event_matches(
                        event,
                        &event.economy_event_id,
                        &delivery.matrix_user_id,
                        "work_delivered",
                        &delivery.work_order_id,
                        0,
                        reputation_delta,
                        delivery.created_at_epoch,
                    )
            })
            .cloned();
        let delivery_projection_already_applied = existing_economy_event.is_some();
        let delivery_event_present = league.world.world_economy_events.iter().any(|event| {
            event.economy_event_id == delivery_event_id
                || event.economy_event_id == legacy_delivery_event_id
        });
        let delivery_event_poisoned =
            delivery_event_present && !delivery_projection_already_applied;
        let terminal_delivery_replay = existing_delivery
            .as_ref()
            .is_some_and(|existing| existing.status == "delivered");
        if reputation_delta > 0
            && !delivery_projection_already_applied
            && !delivery_event_poisoned
            && !terminal_delivery_replay
        {
            if let Some(company_index) = indexes
                .company_index_by_id
                .get(&work_order_seed.company_id)
                .copied()
            {
                if let Some(company) = league.world.world_companies.get_mut(company_index) {
                    company.reputation_score =
                        company.reputation_score.saturating_add(reputation_delta);
                }
            }
            let mut seller = ensure_league_player(&mut league, &matrix_user_id, None);
            seller.xp = seller.xp.saturating_add(delivery.score.round() as i64);
            seller.reputation = seller.reputation.saturating_add(reputation_delta);
            seller.rating = seller
                .rating
                .saturating_add(((delivery.score - 50.0) / 6.0).round() as i64);
            league
                .players_by_matrix_user
                .insert(matrix_user_id.clone(), seller);
        }
        let standing = if reputation_delta > 0
            && !delivery_projection_already_applied
            && !delivery_event_poisoned
            && !terminal_delivery_replay
        {
            Some(upsert_world_faction_standing(
                &mut league,
                &matrix_user_id,
                faction_id,
                reputation_delta,
                delivery.created_at_epoch,
            ))
        } else if reputation_delta > 0 {
            indexes
                .faction_standing_index(&matrix_user_id, faction_id)
                .and_then(|index| league.world.world_faction_standings.get(index).cloned())
        } else {
            None
        };
        let economy_event = if reputation_delta > 0 {
            Some(WorldEconomyEvent {
                economy_event_id: delivery_event_id.clone(),
                matrix_user_id: delivery.matrix_user_id.clone(),
                event_kind: "work_delivered".to_string(),
                subject_id: delivery.work_order_id.clone(),
                credits_delta: 0,
                reputation_delta,
                created_at_epoch: delivery.created_at_epoch,
            })
        } else {
            None
        };
        let economy_event = if delivery_projection_already_applied {
            existing_economy_event.clone()
        } else if delivery_event_poisoned {
            None
        } else {
            if let Some(economy_event) = economy_event.clone() {
                push_world_economy_event_once(&mut league.world, economy_event);
            }
            economy_event
        };
        if let Some(index) = league
            .world
            .world_work_deliveries
            .iter()
            .position(|existing| existing.delivery_id == delivery.delivery_id)
        {
            // Preserve the first delivered judge result; review-hold rows may be re-evaluated in
            // place using the same deterministic identity.
            if league.world.world_work_deliveries[index].status != "delivered" {
                league.world.world_work_deliveries[index] = delivery.clone();
            }
        } else {
            league.world.world_work_deliveries.push(delivery.clone());
        }
        (
            league.clone(),
            league.world.world_work_orders[work_index].clone(),
            delivery,
            economy_event,
            standing,
        )
    };
    if let Err(response) =
        persist_league_state_after_command(&state, &snapshot.0, "world_work_deliver").await
    {
        return response;
    }
    let WorldRouteArtifacts {
        preview: route_preview,
        task_graph: route_task_graph,
        ..
    } = build_world_route_artifacts(&snapshot.0.world);
    (
        StatusCode::OK,
        Json(json!({
            "kind": "trillionnium_world_work_delivery",
            "world": "trillionnium_world",
            "work_order": snapshot.1,
            "delivery": snapshot.2,
            "economy_event": snapshot.3,
            "standing": snapshot.4,
            "route_preview": route_preview,
            "route_task_graph": route_task_graph,
        })),
    )
        .into_response()
}

pub(super) async fn deliver_world_work_order(
    Path(work_order_id): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<WorldWorkDeliverRequest>,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }
    deliver_world_work_order_inner(state, work_order_id, payload).await
}

pub(super) async fn post_world_web_work_deliver(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(payload): Form<WorldWebWorkDeliverRequest>,
) -> Response {
    let web_session = match authorize_league_web_session(&state, &headers, payload.csrf.as_deref())
    {
        Ok(value) => value,
        Err(response) => {
            if matches!(state.config().runtime_profile, RuntimeProfile::LocalDev)
                && cookie_value(&headers, &state.config().league_web_session_cookie_name).is_none()
            {
                None
            } else {
                return response;
            }
        }
    };
    let matrix_user_id = web_session
        .as_ref()
        .map(|session| session.matrix_user_id.clone())
        .or_else(|| {
            normalize_league_matrix_user(
                payload
                    .matrix_user_id
                    .as_deref()
                    .unwrap_or("@alice:local.dev"),
            )
        })
        .unwrap_or_else(|| "@alice:local.dev".to_string());
    let work_order_id = payload
        .work_order_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("latest")
        .to_string();
    let response = deliver_world_work_order_inner(
        state,
        work_order_id,
        WorldWorkDeliverRequest {
            matrix_user_id,
            room_id: web_session
                .as_ref()
                .and_then(|session| session.room_id.clone())
                .or_else(|| Some("!web-local:local.dev".to_string())),
            body: payload
                .body
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or("Work delivery package: deliverable, evidence, acceptance checklist, risk review, next action, and self-review.")
                .to_string(),
        },
    )
    .await;
    if response.status().is_success() {
        Redirect::to("/world?work=delivered").into_response()
    } else {
        response
    }
}

pub(super) async fn accept_world_work_order_inner(
    state: AppState,
    work_order_id: String,
    payload: WorldWorkAcceptRequest,
) -> Response {
    let matrix_user_id = match normalize_league_matrix_user(&payload.matrix_user_id) {
        Some(value) => value,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "matrix_user_id is required" })),
            )
                .into_response()
        }
    };
    let body = match validate_text_payload(&payload.body, state.config().max_text_chars) {
        Ok(value) => value,
        Err(response) => return response,
    };
    let snapshot = {
        let now = Utc::now().timestamp();
        let mut league = state.inner.league_state.lock().await;
        let indexes = build_world_indexes(&league.world);
        let Some(work_index) =
            indexes.resolve_acceptable_work_order_index(&work_order_id, &matrix_user_id)
        else {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "world work order not found", "work_order_id": work_order_id })),
            )
                .into_response();
        };
        if league.world.world_work_orders[work_index].buyer_matrix_user_id != matrix_user_id {
            return (
                StatusCode::FORBIDDEN,
                Json(json!({ "error": "work order belongs to another buyer", "work_order_id": work_order_id })),
            )
                .into_response();
        }
        let work_order_seed = league.world.world_work_orders[work_index].clone();
        // Acceptance identity is derived from the immutable command body, not wall-clock time.
        // This lets a retry reuse the same consume intent and projection key after a lost
        // response.  A different body for the same work order is a collision, not a new payout.
        let existing_acceptance = league
            .world
            .world_work_acceptances
            .iter()
            .rev()
            .find(|acceptance| {
                acceptance.work_order_id == work_order_seed.work_order_id
                    && acceptance.matrix_user_id == matrix_user_id
                    && acceptance.body == body
            })
            .cloned();
        let has_conflicting_acceptance =
            league
                .world
                .world_work_acceptances
                .iter()
                .any(|acceptance| {
                    acceptance.work_order_id == work_order_seed.work_order_id
                        && acceptance.matrix_user_id == matrix_user_id
                        && acceptance.body != body
                        && !matches!(acceptance.status.as_str(), "accepted" | "completed")
                });
        if has_conflicting_acceptance {
            return (
                StatusCode::CONFLICT,
                Json(json!({
                    "error": "work order already has a pending acceptance with a different body",
                    "work_order_id": work_order_id,
                })),
            )
                .into_response();
        }
        if existing_acceptance.is_none() && work_order_seed.status != "delivered" {
            return (
                StatusCode::CONFLICT,
                Json(json!({ "error": "work order is not ready for acceptance", "status": work_order_seed.status, "work_order_id": work_order_id })),
            )
                .into_response();
        }
        let Some(purchase_index) = indexes
            .purchase_index_by_id
            .get(&work_order_seed.purchase_id)
            .copied()
        else {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "world purchase not found for work order", "work_order_id": work_order_id })),
            )
                .into_response();
        };
        let purchase_seed = league.world.world_purchases[purchase_index].clone();
        if !world_purchase_seller_settlement_active(&league.world, &purchase_seed) {
            return (
                StatusCode::CONFLICT,
                Json(json!({
                    "error": "world purchase seller settlement is not active",
                    "work_order_id": work_order_id,
                    "purchase_id": purchase_seed.purchase_id,
                    "purchase_status": purchase_seed.status,
                    "ledger_status": purchase_seed.ledger_status,
                })),
            )
                .into_response();
        }
        let reputation_delta = (work_order_seed.value_score / 20).max(1);
        let acceptance = existing_acceptance.unwrap_or_else(|| WorldWorkAcceptance {
            acceptance_id: league_hash_id(
                "world-acceptance",
                &format!(
                    "{}:{}:{}",
                    work_order_seed.work_order_id, matrix_user_id, body
                ),
            ),
            work_order_id: work_order_seed.work_order_id.clone(),
            matrix_user_id: matrix_user_id.clone(),
            body: body.clone(),
            status: "pending_consume".to_string(),
            reputation_delta,
            created_at_epoch: now,
        });
        if !matches!(acceptance.status.as_str(), "accepted" | "completed") {
            league.world.world_work_orders[work_index].status =
                "accepted_pending_payment".to_string();
        }
        if !league
            .world
            .world_work_acceptances
            .iter()
            .any(|candidate| candidate.acceptance_id == acceptance.acceptance_id)
        {
            league.world.world_work_acceptances.push(acceptance.clone());
        }
        (
            league.clone(),
            league.world.world_work_orders[work_index].clone(),
            purchase_seed,
            acceptance,
            reputation_delta,
        )
    };
    // Persist the stable acceptance/consume identity before crossing the Ledger boundary.  A
    // crash after consume but before the final response can then resume the same operation rather
    // than creating another acceptance and another buyer projection.
    if let Err(response) =
        persist_league_state_after_command(&state, &snapshot.0, "world_work_accept").await
    {
        return response;
    }
    let buyer_consume =
        consume_world_purchase_with_ledger(&state, payload.room_id.as_deref(), &snapshot.2).await;
    let buyer_consume_receipt = buyer_consume.term_exchange_receipt.clone();
    let final_snapshot = {
        let mut league = state.inner.league_state.lock().await;
        record_world_term_exchange_receipt(&mut league.world, buyer_consume_receipt);
        let indexes = build_world_indexes(&league.world);
        let mut work_order = snapshot.1.clone();
        let mut purchase = snapshot.2.clone();
        let mut acceptance = snapshot.3.clone();
        let mut economy_event = None;
        let mut standing = None;
        let expected_buyer_amount = world_purchase_buyer_amount(&purchase);
        let buyer_consume_amount_matches = expected_buyer_amount
            .is_some_and(|expected| buyer_consume.amount_credits == Some(expected));
        let remote_buyer_consumed = buyer_consume
            .progression_allowed_for_term("world_commerce_purchase", &["consumed", "duplicate"])
            && buyer_consume_amount_matches;
        let self_dealing_work = work_order.buyer_matrix_user_id == work_order.seller_matrix_user_id;
        let accepted_event_id = league_hash_id(
            "world-econ",
            &format!(
                "{}:{}:{}",
                work_order.buyer_matrix_user_id,
                acceptance.acceptance_id,
                acceptance.created_at_epoch
            ),
        );
        let acceptance_projection_marker = if self_dealing_work {
            None
        } else {
            world_acceptance_projection_marker(
                &league.world,
                &purchase,
                &accepted_event_id,
                &work_order.seller_matrix_user_id,
                &work_order.work_order_id,
                snapshot.4,
                acceptance.created_at_epoch,
            )
        };
        let projection_already_applied = acceptance_projection_marker == Some(true);
        let projection_event_present = acceptance_projection_marker.is_some();
        let projection_event_poisoned = acceptance_projection_marker == Some(false);
        // A poisoned compatibility marker must not turn a successful-looking retry into a
        // terminal projection.  Keep the operation retryable until the immutable event/receipt
        // identity is repaired or a fresh exact consume response can be reconciled safely.
        let buyer_consumed =
            !projection_event_poisoned && (remote_buyer_consumed || projection_already_applied);
        // A replay that cannot reach Ledger must not regress a previously accepted operation.
        if !projection_already_applied || remote_buyer_consumed {
            purchase.buyer_consume_status = Some(buyer_consume.status.clone());
            purchase.buyer_consume_entry_id = buyer_consume.entry_id.clone();
            purchase.buyer_consume_balance_after = buyer_consume.balance_after;
            purchase.buyer_consume_error = buyer_consume.error.clone().or_else(|| {
                (!buyer_consume_amount_matches
                    && buyer_consume.progression_allowed_for_term(
                        "world_commerce_purchase",
                        &["consumed", "duplicate"],
                    ))
                .then(|| {
                    format!(
                        "buyer consume amount mismatch: expected {:?}, got {:?}",
                        expected_buyer_amount, buyer_consume.amount_credits
                    )
                })
            });
        }
        purchase.status = if buyer_consumed {
            "completed".to_string()
        } else if buyer_consume.status.starts_with("skipped") {
            "accepted_payment_hold".to_string()
        } else {
            "accepted_payment_failed".to_string()
        };
        work_order.status = if buyer_consumed {
            "completed".to_string()
        } else if buyer_consume.status.starts_with("skipped") {
            "accepted_payment_hold".to_string()
        } else {
            "accepted_payment_failed".to_string()
        };
        acceptance.status = if buyer_consumed {
            "accepted".to_string()
        } else if buyer_consume.status.starts_with("skipped") {
            "accepted_payment_hold".to_string()
        } else {
            "accepted_payment_failed".to_string()
        };
        indexes.replace_purchase_by_id(&mut league.world, &purchase);
        indexes.replace_work_order_by_id(&mut league.world, &work_order);
        indexes.replace_acceptance_by_id(&mut league.world, &acceptance);
        if buyer_consumed
            && !self_dealing_work
            && !projection_already_applied
            && !projection_event_present
            && !projection_event_poisoned
        {
            if let Some(company_index) = indexes
                .company_index_by_id
                .get(&work_order.company_id)
                .copied()
            {
                if let Some(company) = league.world.world_companies.get_mut(company_index) {
                    company.reputation_score = company.reputation_score.saturating_add(snapshot.4);
                    company.level = 1_i64.saturating_add((company.revenue_score / 100).max(0));
                }
            }
            let mut buyer =
                ensure_league_player(&mut league, &work_order.buyer_matrix_user_id, None);
            buyer.xp = buyer.xp.saturating_add(3);
            buyer.reputation = buyer.reputation.saturating_add(1);
            league
                .players_by_matrix_user
                .insert(work_order.buyer_matrix_user_id.clone(), buyer);
            let mut seller =
                ensure_league_player(&mut league, &work_order.seller_matrix_user_id, None);
            seller.xp = seller.xp.saturating_add(snapshot.4);
            seller.reputation = seller.reputation.saturating_add(snapshot.4);
            seller.rating = seller.rating.saturating_add((snapshot.4 / 2).max(1));
            league
                .players_by_matrix_user
                .insert(work_order.seller_matrix_user_id.clone(), seller);
            let accepted_event = WorldEconomyEvent {
                economy_event_id: accepted_event_id.clone(),
                matrix_user_id: work_order.seller_matrix_user_id.clone(),
                event_kind: "work_accepted".to_string(),
                subject_id: work_order.work_order_id.clone(),
                credits_delta: 0,
                reputation_delta: snapshot.4,
                created_at_epoch: acceptance.created_at_epoch,
            };
            standing = Some(upsert_world_faction_standing(
                &mut league,
                &work_order.seller_matrix_user_id,
                "faction-market-guild",
                snapshot.4,
                acceptance.created_at_epoch,
            ));
            push_world_economy_event_once(&mut league.world, accepted_event.clone());
            economy_event = Some(accepted_event);
        } else if buyer_consumed && !self_dealing_work && projection_already_applied {
            economy_event = league
                .world
                .world_economy_events
                .iter()
                .find(|event| event.economy_event_id == accepted_event_id)
                .cloned();
            standing = indexes
                .faction_standing_index(&work_order.seller_matrix_user_id, "faction-market-guild")
                .and_then(|index| league.world.world_faction_standings.get(index).cloned());
        }
        (
            league.clone(),
            work_order,
            purchase,
            acceptance,
            economy_event,
            standing,
        )
    };
    if let Err(response) =
        persist_league_state_after_command(&state, &final_snapshot.0, "world_work_accept").await
    {
        return response;
    }
    let WorldRouteArtifacts {
        preview: route_preview,
        task_graph: route_task_graph,
        ..
    } = build_world_route_artifacts(&final_snapshot.0.world);
    (
        StatusCode::OK,
        Json(json!({
            "kind": "trillionnium_world_work_acceptance",
            "world": "trillionnium_world",
            "work_order": final_snapshot.1,
            "purchase": final_snapshot.2,
            "acceptance": final_snapshot.3,
            "economy_event": final_snapshot.4,
            "standing": final_snapshot.5,
            "buyer_consume_status": buyer_consume.status,
            "buyer_consume_entry_id": buyer_consume.entry_id,
            "buyer_consume_error": buyer_consume.error,
            "route_preview": route_preview,
            "route_task_graph": route_task_graph,
        })),
    )
        .into_response()
}

pub(super) async fn accept_world_work_order(
    Path(work_order_id): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<WorldWorkAcceptRequest>,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }
    accept_world_work_order_inner(state, work_order_id, payload).await
}

pub(super) async fn post_world_web_work_accept(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(payload): Form<WorldWebWorkAcceptRequest>,
) -> Response {
    let web_session = match authorize_league_web_session(&state, &headers, payload.csrf.as_deref())
    {
        Ok(value) => value,
        Err(response) => {
            if matches!(state.config().runtime_profile, RuntimeProfile::LocalDev)
                && cookie_value(&headers, &state.config().league_web_session_cookie_name).is_none()
            {
                None
            } else {
                return response;
            }
        }
    };
    let matrix_user_id = web_session
        .as_ref()
        .map(|session| session.matrix_user_id.clone())
        .or_else(|| {
            normalize_league_matrix_user(
                payload
                    .matrix_user_id
                    .as_deref()
                    .unwrap_or("@alice:local.dev"),
            )
        })
        .unwrap_or_else(|| "@alice:local.dev".to_string());
    let work_order_id = payload
        .work_order_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("latest")
        .to_string();
    let response = accept_world_work_order_inner(
        state,
        work_order_id,
        WorldWorkAcceptRequest {
            matrix_user_id,
            room_id: web_session
                .as_ref()
                .and_then(|session| session.room_id.clone())
                .or_else(|| Some("!web-local:local.dev".to_string())),
            body: payload
                .body
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or("Buyer acceptance: confirm customer deliverable, evidence package, quality note, risk controls, next collaboration, reputation confirmation, and self-review.")
                .to_string(),
        },
    )
    .await;
    if response.status().is_success() {
        Redirect::to("/world?work=accepted").into_response()
    } else {
        response
    }
}

pub(super) async fn reject_world_work_order_inner(
    state: AppState,
    work_order_id: String,
    payload: WorldWorkRejectRequest,
) -> Response {
    let matrix_user_id = match normalize_league_matrix_user(&payload.matrix_user_id) {
        Some(value) => value,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "matrix_user_id is required" })),
            )
                .into_response()
        }
    };
    let body = match validate_text_payload(&payload.body, state.config().max_text_chars) {
        Ok(value) => value,
        Err(response) => return response,
    };
    let snapshot = {
        let now = Utc::now().timestamp();
        let mut league = state.inner.league_state.lock().await;
        let indexes = build_world_indexes(&league.world);
        let Some(work_index) =
            indexes.resolve_rejectable_work_order_index(&work_order_id, &matrix_user_id)
        else {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "world work order not found", "work_order_id": work_order_id })),
            )
                .into_response();
        };
        if league.world.world_work_orders[work_index].buyer_matrix_user_id != matrix_user_id {
            return (
                StatusCode::FORBIDDEN,
                Json(json!({ "error": "work order belongs to another buyer", "work_order_id": work_order_id })),
            )
                .into_response();
        }
        if !matches!(
            league.world.world_work_orders[work_index].status.as_str(),
            "delivered"
                | "delivery_review_hold"
                | "rejected_refund_hold"
                | "rejected_refund_failed"
                | "rejected_chargeback_failed"
                | "rejected_pending_refund"
                | "rejected_pending_chargeback"
        ) {
            return (
                StatusCode::CONFLICT,
                Json(json!({ "error": "work order is not rejectable", "status": league.world.world_work_orders[work_index].status, "work_order_id": work_order_id })),
            )
                .into_response();
        }
        let work_order_seed = league.world.world_work_orders[work_index].clone();
        let Some(purchase_index) = indexes
            .purchase_index_by_id
            .get(&work_order_seed.purchase_id)
            .copied()
        else {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "world purchase not found for work order", "work_order_id": work_order_id })),
            )
                .into_response();
        };
        let purchase_seed = league.world.world_purchases[purchase_index].clone();
        let rejection_refund_scope = latest_world_rejection_scope_for_work_order(
            &league.world,
            &work_order_seed.work_order_id,
        );
        let retry_chargeback_only = work_order_seed.status == "rejected_chargeback_failed"
            && world_purchase_buyer_refund_completed(
                &league.world,
                &purchase_seed,
                rejection_refund_scope.as_deref(),
            );
        let retry_status = if retry_chargeback_only {
            "pending_chargeback"
        } else {
            "pending_refund"
        };
        let rejection = if matches!(
            work_order_seed.status.as_str(),
            "rejected_refund_hold"
                | "rejected_refund_failed"
                | "rejected_chargeback_failed"
                | "rejected_pending_refund"
                | "rejected_pending_chargeback"
        ) {
            match league
                .world
                .world_work_rejections
                .iter()
                .rev()
                .find(|rejection| rejection.work_order_id == work_order_seed.work_order_id)
                .cloned()
            {
                Some(mut rejection) => {
                    rejection.body = body;
                    rejection.status = retry_status.to_string();
                    rejection
                }
                None => WorldWorkRejection {
                    rejection_id: league_hash_id(
                        "world-rejection",
                        &format!(
                            "{}:{}:{}",
                            work_order_seed.work_order_id,
                            matrix_user_id,
                            world_rejection_attempt(&league.world, &work_order_seed.work_order_id)
                        ),
                    ),
                    work_order_id: work_order_seed.work_order_id.clone(),
                    matrix_user_id: matrix_user_id.clone(),
                    body,
                    status: retry_status.to_string(),
                    refund_status: "pending".to_string(),
                    created_at_epoch: now,
                },
            }
        } else {
            WorldWorkRejection {
                rejection_id: league_hash_id(
                    "world-rejection",
                    &format!(
                        "{}:{}:{}",
                        work_order_seed.work_order_id,
                        matrix_user_id,
                        world_rejection_attempt(&league.world, &work_order_seed.work_order_id)
                    ),
                ),
                work_order_id: work_order_seed.work_order_id.clone(),
                matrix_user_id: matrix_user_id.clone(),
                body,
                status: retry_status.to_string(),
                refund_status: "pending".to_string(),
                created_at_epoch: now,
            }
        };
        league.world.world_work_orders[work_index].status = if retry_chargeback_only {
            "rejected_pending_chargeback".to_string()
        } else {
            "rejected_pending_refund".to_string()
        };
        if indexes
            .rejection_index_by_id
            .contains_key(&rejection.rejection_id)
        {
            indexes.replace_rejection_by_id(&mut league.world, &rejection);
        } else {
            league.world.world_work_rejections.push(rejection.clone());
        }
        (
            league.clone(),
            league.world.world_work_orders[work_index].clone(),
            purchase_seed,
            rejection,
            retry_chargeback_only,
        )
    };
    // Persist the pending rejection operation before touching the remote Ledger.  A process crash
    // or lost response can then resume the same rejection_id/intent instead of minting a second
    // refund or seller chargeback.
    if let Err(response) =
        persist_league_state_after_command(&state, &snapshot.0, "world_work_reject").await
    {
        return response;
    }
    let buyer_refund = if snapshot.4 {
        world_replay_buyer_refund_settlement(
            &snapshot.0.world,
            &snapshot.2,
            &snapshot.3.rejection_id,
        )
        .unwrap_or_else(|| LeagueLedgerSettlement {
            status: "reconcile_required".to_string(),
            error: Some(
                "prior buyer refund lacks an exact typed receipt; seller chargeback is blocked"
                    .to_string(),
            ),
            ..Default::default()
        })
    } else {
        refund_world_purchase_with_ledger(
            &state,
            payload.room_id.as_deref(),
            &snapshot.2,
            &snapshot.3.rejection_id,
        )
        .await
    };
    let expected_buyer_amount = world_purchase_buyer_amount(&snapshot.2);
    let buyer_refund_amount_matches =
        expected_buyer_amount.is_some_and(|expected| buyer_refund.amount_credits == Some(expected));
    let buyer_refunded = buyer_refund
        .progression_allowed_for_term("world_commerce_purchase", &["refunded", "duplicate"])
        && buyer_refund_amount_matches;
    let seller_chargeback = if buyer_refunded {
        chargeback_world_purchase_seller_with_ledger(
            &state,
            payload.room_id.as_deref(),
            &snapshot.2,
            &snapshot.3.rejection_id,
        )
        .await
    } else {
        LeagueLedgerSettlement {
            status: "skipped_buyer_not_refunded".to_string(),
            error: Some(
                "seller chargeback skipped because buyer refund did not complete".to_string(),
            ),
            ..Default::default()
        }
    };
    let buyer_refund_receipt = buyer_refund.term_exchange_receipt.clone();
    let seller_chargeback_receipt = seller_chargeback.term_exchange_receipt.clone();
    let final_snapshot = {
        let mut league = state.inner.league_state.lock().await;
        record_world_term_exchange_receipt(&mut league.world, buyer_refund_receipt);
        record_world_term_exchange_receipt(&mut league.world, seller_chargeback_receipt);
        let indexes = build_world_indexes(&league.world);
        let mut work_order = snapshot.1.clone();
        let mut purchase = snapshot.2.clone();
        let mut rejection = snapshot.3.clone();
        let mut economy_event = None;
        let mut standing = None;
        let expected_seller_net_credits =
            world_seller_net_credits_for_price(purchase.price_credits);
        let seller_charged_back = buyer_refunded
            && seller_chargeback.progression_allowed_for_term(
                "world_commerce_purchase",
                &["seller_chargeback_consumed", "duplicate"],
            )
            && seller_chargeback.amount_credits == Some(expected_seller_net_credits);
        let seller_zero_net_skipped = world_settlement_is_zero_seller_net_skip(
            &seller_chargeback,
            expected_seller_net_credits,
        );
        // A buyer refund is terminally safe when no seller settlement ever became active.  In
        // that case there is no seller value to claw back, and the chargeback helper deliberately
        // returns this typed no-op instead of manufacturing a failed debit.  Treat only this
        // explicit status (with no receipt/amount) as a cleared seller leg; generic `skipped_*`
        // outcomes remain holds and keep the cancellation retryable.
        let seller_not_settled_skipped = buyer_refunded
            && seller_chargeback.status == "skipped_seller_not_settled"
            && seller_chargeback.amount_credits.is_none()
            && seller_chargeback.term_exchange_receipt.is_none();
        let seller_chargeback_cleared = buyer_refunded
            && (seller_charged_back || seller_zero_net_skipped || seller_not_settled_skipped);
        purchase.buyer_consume_status = Some(if buyer_refunded {
            "refunded".to_string()
        } else {
            buyer_refund.status.clone()
        });
        purchase.buyer_consume_entry_id = buyer_refund.entry_id.clone();
        purchase.buyer_consume_balance_after = buyer_refund.balance_after;
        purchase.buyer_consume_error = buyer_refund.error.clone().or_else(|| {
            (buyer_refund.progression_allowed_for_term(
                "world_commerce_purchase",
                &["refunded", "duplicate"],
            ) && !buyer_refund_amount_matches)
                .then(|| {
                    format!(
                        "buyer refund amount mismatch: expected {:?}, got {:?}",
                        expected_buyer_amount, buyer_refund.amount_credits
                    )
                })
        });
        purchase.status = if buyer_refunded {
            if seller_chargeback_cleared {
                "rejected_refunded".to_string()
            } else {
                "rejected_chargeback_failed".to_string()
            }
        } else if buyer_refund.status.starts_with("skipped") {
            "rejected_refund_hold".to_string()
        } else {
            "rejected_refund_failed".to_string()
        };
        if buyer_refunded {
            purchase.ledger_status = Some(if seller_charged_back {
                "seller_chargeback_consumed".to_string()
            } else if seller_zero_net_skipped {
                seller_chargeback.status.clone()
            } else {
                "seller_chargeback_failed".to_string()
            });
            purchase.ledger_entry_id = seller_chargeback
                .entry_id
                .clone()
                .or_else(|| purchase.ledger_entry_id.clone());
            purchase.ledger_balance_after = seller_chargeback
                .balance_after
                .or(purchase.ledger_balance_after);
            purchase.ledger_error = seller_chargeback.error.clone();
            if seller_chargeback_cleared {
                let rejected_event = WorldEconomyEvent {
                    economy_event_id: league_hash_id(
                        "world-econ",
                        &format!(
                            "{}:{}:{}",
                            rejection.matrix_user_id,
                            rejection.rejection_id,
                            rejection.created_at_epoch
                        ),
                    ),
                    matrix_user_id: rejection.matrix_user_id.clone(),
                    event_kind: "work_rejected".to_string(),
                    subject_id: work_order.work_order_id.clone(),
                    credits_delta: purchase.price_credits.saturating_neg(),
                    reputation_delta: 0,
                    created_at_epoch: rejection.created_at_epoch,
                };
                if push_world_economy_event_once(&mut league.world, rejected_event.clone()) {
                    standing = Some(upsert_world_faction_standing(
                        &mut league,
                        &rejection.matrix_user_id,
                        "faction-market-guild",
                        1,
                        rejection.created_at_epoch,
                    ));
                }
                economy_event = Some(rejected_event);
            }
            if seller_charged_back {
                let seller_net_credits = seller_chargeback
                    .amount_credits
                    .expect("charged-back seller settlement must carry exact amount_credits");
                let chargeback_event = WorldEconomyEvent {
                    economy_event_id: league_hash_id(
                        "world-seller-chargeback",
                        &format!(
                            "{}:{}:{}",
                            purchase.seller_matrix_user_id, rejection.rejection_id, "projection"
                        ),
                    ),
                    matrix_user_id: purchase.seller_matrix_user_id.clone(),
                    event_kind: "seller_chargeback".to_string(),
                    subject_id: purchase.purchase_id.clone(),
                    credits_delta: seller_net_credits.saturating_neg(),
                    reputation_delta: 0,
                    created_at_epoch: rejection.created_at_epoch,
                };
                // The event id is the projection id.  Only the first append may mutate the
                // compatibility earned_credits balance; retries replay the Ledger receipt but do
                // not debit the seller again.
                if push_world_economy_event_once(&mut league.world, chargeback_event) {
                    if let Some(player) = league
                        .players_by_matrix_user
                        .get_mut(&purchase.seller_matrix_user_id)
                    {
                        if let Some(seller_net_display) =
                            exact_credits_to_legacy_display(seller_net_credits)
                        {
                            // Compatibility projection only: subtract the exact chargeback amount.
                            player.earned_credits =
                                (player.earned_credits - seller_net_display).max(0.0);
                        }
                    }
                }
            }
        }
        work_order.status = purchase.status.clone();
        rejection.refund_status = buyer_refund.status.clone();
        rejection.status = purchase.status.clone();
        indexes.replace_purchase_by_id(&mut league.world, &purchase);
        indexes.replace_work_order_by_id(&mut league.world, &work_order);
        indexes.replace_rejection_by_id(&mut league.world, &rejection);
        (
            league.clone(),
            work_order,
            purchase,
            rejection,
            economy_event,
            standing,
        )
    };
    if let Err(response) =
        persist_league_state_after_command(&state, &final_snapshot.0, "world_work_reject").await
    {
        return response;
    }
    let WorldRouteArtifacts {
        preview: route_preview,
        task_graph: route_task_graph,
        ..
    } = build_world_route_artifacts(&final_snapshot.0.world);
    (
        StatusCode::OK,
        Json(json!({
            "kind": "trillionnium_world_work_rejection",
            "world": "trillionnium_world",
            "work_order": final_snapshot.1,
            "purchase": final_snapshot.2,
            "rejection": final_snapshot.3,
            "economy_event": final_snapshot.4,
            "standing": final_snapshot.5,
            "buyer_refund_status": buyer_refund.status,
            "buyer_refund_entry_id": buyer_refund.entry_id,
            "buyer_refund_error": buyer_refund.error,
            "seller_chargeback_status": seller_chargeback.status,
            "seller_chargeback_entry_id": seller_chargeback.entry_id,
            "seller_chargeback_error": seller_chargeback.error,
            "route_preview": route_preview,
            "route_task_graph": route_task_graph,
        })),
    )
        .into_response()
}

pub(super) async fn reject_world_work_order(
    Path(work_order_id): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<WorldWorkRejectRequest>,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }
    reject_world_work_order_inner(state, work_order_id, payload).await
}

pub(super) async fn post_world_web_work_reject(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(payload): Form<WorldWebWorkRejectRequest>,
) -> Response {
    let web_session = match authorize_league_web_session(&state, &headers, payload.csrf.as_deref())
    {
        Ok(value) => value,
        Err(response) => {
            if matches!(state.config().runtime_profile, RuntimeProfile::LocalDev)
                && cookie_value(&headers, &state.config().league_web_session_cookie_name).is_none()
            {
                None
            } else {
                return response;
            }
        }
    };
    let matrix_user_id = web_session
        .as_ref()
        .map(|session| session.matrix_user_id.clone())
        .or_else(|| {
            normalize_league_matrix_user(
                payload
                    .matrix_user_id
                    .as_deref()
                    .unwrap_or("@alice:local.dev"),
            )
        })
        .unwrap_or_else(|| "@alice:local.dev".to_string());
    let work_order_id = payload
        .work_order_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("latest")
        .to_string();
    let response = reject_world_work_order_inner(
        state,
        work_order_id,
        WorldWorkRejectRequest {
            matrix_user_id,
            room_id: web_session
                .as_ref()
                .and_then(|session| session.room_id.clone())
                .or_else(|| Some("!web-local:local.dev".to_string())),
            body: payload
                .body
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or("Buyer rejection: delivery is not accepted; record customer deliverable gap, evidence package, refund risk controls, revision requirements, next action, and self-review.")
                .to_string(),
        },
    )
    .await;
    if response.status().is_success() {
        Redirect::to("/world?work=rejected").into_response()
    } else {
        response
    }
}

pub(super) async fn reopen_world_work_order_inner(
    state: AppState,
    work_order_id: String,
    payload: WorldWorkReopenRequest,
) -> Response {
    let matrix_user_id = match normalize_league_matrix_user(&payload.matrix_user_id) {
        Some(value) => value,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "matrix_user_id is required" })),
            )
                .into_response()
        }
    };
    let body = match validate_text_payload(&payload.body, state.config().max_text_chars) {
        Ok(value) => value,
        Err(response) => return response,
    };
    let snapshot = {
        let now = Utc::now().timestamp();
        let mut league = state.inner.league_state.lock().await;
        let indexes = build_world_indexes(&league.world);
        let Some(work_index) =
            indexes.resolve_reopenable_work_order_index(&work_order_id, &matrix_user_id)
        else {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "world work order not found", "work_order_id": work_order_id })),
            )
                .into_response();
        };
        if league.world.world_work_orders[work_index].buyer_matrix_user_id != matrix_user_id {
            return (
                StatusCode::FORBIDDEN,
                Json(json!({ "error": "work order belongs to another buyer", "work_order_id": work_order_id })),
            )
                .into_response();
        }
        if !matches!(
            league.world.world_work_orders[work_index].status.as_str(),
            "rejected_refunded"
                | "rejected_refund_hold"
                | "rejected_refund_failed"
                | "reopen_reserve_hold"
                | "reopen_reserve_failed"
                | "reopen_seller_settlement_pending"
                | "reopen_seller_settlement_failed"
                | "reopen_pending_reserve"
        ) {
            let status = league.world.world_work_orders[work_index].status.clone();
            return (
                StatusCode::CONFLICT,
                Json(json!({ "error": "work order is not reopenable", "status": status, "work_order_id": work_order_id })),
            )
                .into_response();
        }
        let work_order_seed = league.world.world_work_orders[work_index].clone();
        let Some(purchase_index) = indexes
            .purchase_index_by_id
            .get(&work_order_seed.purchase_id)
            .copied()
        else {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "world purchase not found for work order", "work_order_id": work_order_id })),
            )
                .into_response();
        };
        let purchase_seed = league.world.world_purchases[purchase_index].clone();
        if !world_purchase_rejection_settlement_released(
            &league.world,
            &purchase_seed,
            &work_order_seed.work_order_id,
        ) {
            return (
                StatusCode::CONFLICT,
                Json(json!({
                    "error": "world work rejection settlement is not complete",
                    "work_order_id": work_order_id,
                    "purchase_id": purchase_seed.purchase_id,
                    "purchase_status": purchase_seed.status,
                    "buyer_refund_status": purchase_seed.buyer_consume_status,
                    "seller_chargeback_status": purchase_seed.ledger_status,
                })),
            )
                .into_response();
        }
        let retrying_reopen = matches!(
            work_order_seed.status.as_str(),
            "reopen_reserve_hold"
                | "reopen_reserve_failed"
                | "reopen_seller_settlement_pending"
                | "reopen_seller_settlement_failed"
                | "reopen_pending_reserve"
        );
        let reopen = if retrying_reopen {
            league
                .world
                .world_work_reopens
                .iter()
                .rev()
                .find(|reopen| reopen.work_order_id == work_order_seed.work_order_id)
                .cloned()
                .map(|mut reopen| {
                    // Keep the original operation identity across a remote retry; the body is
                    // diagnostic input and must not mint another reserve/settlement intent.
                    reopen.body = body.clone();
                    reopen.status = "pending_reopen_reserve".to_string();
                    reopen.reserve_status = "pending".to_string();
                    reopen
                })
                .unwrap_or_else(|| WorldWorkReopen {
                    reopen_id: league_hash_id(
                        "world-reopen",
                        &format!(
                            "{}:{}:{}",
                            work_order_seed.work_order_id,
                            matrix_user_id,
                            world_reopen_attempt(&league.world, &work_order_seed.work_order_id)
                        ),
                    ),
                    work_order_id: work_order_seed.work_order_id.clone(),
                    matrix_user_id: matrix_user_id.clone(),
                    body: body.clone(),
                    status: "pending_reopen_reserve".to_string(),
                    reserve_status: "pending".to_string(),
                    created_at_epoch: now,
                })
        } else {
            WorldWorkReopen {
                reopen_id: league_hash_id(
                    "world-reopen",
                    &format!(
                        "{}:{}:{}",
                        work_order_seed.work_order_id,
                        matrix_user_id,
                        world_reopen_attempt(&league.world, &work_order_seed.work_order_id)
                    ),
                ),
                work_order_id: work_order_seed.work_order_id.clone(),
                matrix_user_id: matrix_user_id.clone(),
                body,
                status: "pending_reopen_reserve".to_string(),
                reserve_status: "pending".to_string(),
                created_at_epoch: now,
            }
        };
        league.world.world_work_orders[work_index].status = "reopen_pending_reserve".to_string();
        let reopen_index = indexes.reopen_index_by_id.get(&reopen.reopen_id).copied();
        if reopen_index.is_some() {
            indexes.replace_reopen_by_id(&mut league.world, &reopen);
        } else {
            league.world.world_work_reopens.push(reopen.clone());
        }
        (
            league.clone(),
            league.world.world_work_orders[work_index].clone(),
            purchase_seed,
            reopen,
        )
    };
    // The reopen reserve intent must survive before the network call.  This is intentionally the
    // same supported command as the final projection; no separate transient write-set exists.
    if let Err(response) =
        persist_league_state_after_command(&state, &snapshot.0, "world_work_reopen").await
    {
        return response;
    }
    let buyer_reopen_reserve = reserve_reopened_world_purchase_with_ledger(
        &state,
        payload.room_id.as_deref(),
        &snapshot.2,
        &snapshot.3,
    )
    .await;
    let expected_buyer_amount = world_purchase_buyer_amount(&snapshot.2);
    let buyer_reopen_reserve_progression_allowed = buyer_reopen_reserve
        .progression_allowed_for_term("world_commerce_purchase", &["reserved", "duplicate"]);
    let buyer_reopen_reserve_amount_matches = expected_buyer_amount
        .is_some_and(|expected| buyer_reopen_reserve.amount_credits == Some(expected));
    let buyer_reserved =
        buyer_reopen_reserve_progression_allowed && buyer_reopen_reserve_amount_matches;
    let seller_reopen_settlement = if buyer_reserved {
        settle_reopened_world_purchase_with_ledger(
            &state,
            payload.room_id.as_deref(),
            &snapshot.2,
            &snapshot.3,
        )
        .await
    } else {
        LeagueLedgerSettlement {
            status: "skipped_buyer_reopen_reserve".to_string(),
            error: Some(
                "seller reopen settlement skipped because buyer reserve did not complete"
                    .to_string(),
            ),
            ..Default::default()
        }
    };
    let expected_seller_net_credits = world_seller_net_credits_for_price(snapshot.2.price_credits);
    let seller_reopen_progression_allowed = seller_reopen_settlement.progression_allowed_for_term(
        "world_commerce_purchase",
        &["reopened_settled", "duplicate"],
    );
    let seller_reopen_amount_matches =
        seller_reopen_settlement.amount_credits == Some(expected_seller_net_credits);
    let seller_resettled = seller_reopen_progression_allowed && seller_reopen_amount_matches;
    let buyer_reopen_reserve_receipt = buyer_reopen_reserve.term_exchange_receipt.clone();
    let seller_reopen_settlement_receipt = seller_reopen_settlement.term_exchange_receipt.clone();
    let final_snapshot = {
        let mut league = state.inner.league_state.lock().await;
        record_world_term_exchange_receipt(&mut league.world, buyer_reopen_reserve_receipt);
        record_world_term_exchange_receipt(&mut league.world, seller_reopen_settlement_receipt);
        let indexes = build_world_indexes(&league.world);
        let mut work_order = snapshot.1.clone();
        let mut purchase = snapshot.2.clone();
        let mut reopen = snapshot.3.clone();
        let mut economy_event = None;
        let mut standing = None;
        purchase.buyer_ledger_status = Some(if buyer_reserved {
            "reopened_reserved".to_string()
        } else {
            buyer_reopen_reserve.status.clone()
        });
        purchase.buyer_ledger_entry_id = buyer_reopen_reserve.entry_id.clone();
        purchase.buyer_ledger_balance_after = buyer_reopen_reserve.balance_after;
        purchase.buyer_ledger_error = buyer_reopen_reserve.error.clone().or_else(|| {
            (buyer_reopen_reserve_progression_allowed && !buyer_reopen_reserve_amount_matches).then(
                || {
                    format!(
                        "buyer reopen reserve amount mismatch: expected {:?}, got {:?}",
                        expected_buyer_amount, buyer_reopen_reserve.amount_credits
                    )
                },
            )
        });
        if buyer_reserved {
            purchase.ledger_status = Some(seller_reopen_settlement.status.clone());
            purchase.ledger_account_id = seller_reopen_settlement.account_id.clone();
            purchase.ledger_entry_id = seller_reopen_settlement.entry_id.clone();
            purchase.ledger_balance_after = seller_reopen_settlement.balance_after;
            purchase.ledger_error = seller_reopen_settlement.error.clone().or_else(|| {
                (seller_reopen_progression_allowed && !seller_reopen_amount_matches).then(|| {
                    format!(
                        "seller reopen settlement amount mismatch: expected {expected_seller_net_credits}, got {:?}",
                        seller_reopen_settlement.amount_credits
                    )
                })
            });
        }
        purchase.status = if buyer_reserved {
            if seller_resettled {
                "reopened_reserved".to_string()
            } else if seller_reopen_settlement.status.starts_with("skipped") {
                "reopen_seller_settlement_pending".to_string()
            } else {
                "reopen_seller_settlement_failed".to_string()
            }
        } else if buyer_reopen_reserve.status.starts_with("skipped") {
            "reopen_reserve_hold".to_string()
        } else {
            "reopen_reserve_failed".to_string()
        };
        work_order.status = if seller_resettled {
            "open".to_string()
        } else {
            purchase.status.clone()
        };
        reopen.reserve_status = buyer_reopen_reserve.status.clone();
        reopen.status = if buyer_reserved {
            if seller_resettled {
                "reopened".to_string()
            } else if seller_reopen_settlement.status.starts_with("skipped") {
                "reopen_seller_settlement_pending".to_string()
            } else {
                "reopen_seller_settlement_failed".to_string()
            }
        } else if buyer_reopen_reserve.status.starts_with("skipped") {
            "reopen_reserve_hold".to_string()
        } else {
            "reopen_reserve_failed".to_string()
        };
        if seller_resettled {
            let reopened_event = WorldEconomyEvent {
                economy_event_id: league_hash_id(
                    "world-econ",
                    &format!(
                        "{}:{}:{}",
                        reopen.matrix_user_id, reopen.reopen_id, reopen.created_at_epoch
                    ),
                ),
                matrix_user_id: reopen.matrix_user_id.clone(),
                event_kind: "work_reopened".to_string(),
                subject_id: work_order.work_order_id.clone(),
                credits_delta: 0,
                reputation_delta: 1,
                created_at_epoch: reopen.created_at_epoch,
            };
            let event_was_new =
                push_world_economy_event_once(&mut league.world, reopened_event.clone());
            if event_was_new {
                standing = Some(upsert_world_faction_standing(
                    &mut league,
                    &reopen.matrix_user_id,
                    "faction-market-guild",
                    1,
                    reopen.created_at_epoch,
                ));
            }
            economy_event = Some(reopened_event);
        }
        indexes.replace_purchase_by_id(&mut league.world, &purchase);
        indexes.replace_work_order_by_id(&mut league.world, &work_order);
        indexes.replace_reopen_by_id(&mut league.world, &reopen);
        (
            league.clone(),
            work_order,
            purchase,
            reopen,
            economy_event,
            standing,
        )
    };
    if let Err(response) =
        persist_league_state_after_command(&state, &final_snapshot.0, "world_work_reopen").await
    {
        return response;
    }
    let WorldRouteArtifacts {
        preview: route_preview,
        task_graph: route_task_graph,
        ..
    } = build_world_route_artifacts(&final_snapshot.0.world);
    (
        StatusCode::OK,
        Json(json!({
            "kind": "trillionnium_world_work_reopen",
            "world": "trillionnium_world",
            "work_order": final_snapshot.1,
            "purchase": final_snapshot.2,
            "reopen": final_snapshot.3,
            "economy_event": final_snapshot.4,
            "standing": final_snapshot.5,
            "buyer_reopen_reserve_status": buyer_reopen_reserve.status,
            "buyer_reopen_reserve_entry_id": buyer_reopen_reserve.entry_id,
            "buyer_reopen_reserve_error": buyer_reopen_reserve.error,
            "seller_reopen_settlement_status": seller_reopen_settlement.status,
            "seller_reopen_settlement_entry_id": seller_reopen_settlement.entry_id,
            "seller_reopen_settlement_error": seller_reopen_settlement.error,
            "route_preview": route_preview,
            "route_task_graph": route_task_graph,
        })),
    )
        .into_response()
}

pub(super) async fn reopen_world_work_order(
    Path(work_order_id): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<WorldWorkReopenRequest>,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }
    reopen_world_work_order_inner(state, work_order_id, payload).await
}

pub(super) async fn post_world_web_work_reopen(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(payload): Form<WorldWebWorkReopenRequest>,
) -> Response {
    let web_session = match authorize_league_web_session(&state, &headers, payload.csrf.as_deref())
    {
        Ok(value) => value,
        Err(response) => {
            if matches!(state.config().runtime_profile, RuntimeProfile::LocalDev)
                && cookie_value(&headers, &state.config().league_web_session_cookie_name).is_none()
            {
                None
            } else {
                return response;
            }
        }
    };
    let matrix_user_id = web_session
        .as_ref()
        .map(|session| session.matrix_user_id.clone())
        .or_else(|| {
            normalize_league_matrix_user(
                payload
                    .matrix_user_id
                    .as_deref()
                    .unwrap_or("@alice:local.dev"),
            )
        })
        .unwrap_or_else(|| "@alice:local.dev".to_string());
    let work_order_id = payload
        .work_order_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("latest")
        .to_string();
    let response = reopen_world_work_order_inner(
        state,
        work_order_id,
        WorldWorkReopenRequest {
            matrix_user_id,
            room_id: web_session
                .as_ref()
                .and_then(|session| session.room_id.clone())
                .or_else(|| Some("!web-local:local.dev".to_string())),
            body: payload
                .body
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or("Buyer reopen: reserve funds again, list customer deliverable revisions, evidence gaps, risk controls, acceptance standard, next redelivery action, and self-review.")
                .to_string(),
        },
    )
    .await;
    if response.status().is_success() {
        Redirect::to("/world?work=reopened").into_response()
    } else {
        response
    }
}

pub(super) async fn cancel_world_work_order_inner(
    state: AppState,
    work_order_id: String,
    payload: WorldWorkCancelRequest,
) -> Response {
    let matrix_user_id = match normalize_league_matrix_user(&payload.matrix_user_id) {
        Some(value) => value,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "matrix_user_id is required" })),
            )
                .into_response()
        }
    };
    let body = match validate_text_payload(&payload.body, state.config().max_text_chars) {
        Ok(value) => value,
        Err(response) => return response,
    };
    let snapshot = {
        let now = Utc::now().timestamp();
        let mut league = state.inner.league_state.lock().await;
        let indexes = build_world_indexes(&league.world);
        let Some(work_index) =
            indexes.resolve_cancellable_work_order_index(&work_order_id, &matrix_user_id)
        else {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "world work order not found", "work_order_id": work_order_id })),
            )
                .into_response();
        };
        if league.world.world_work_orders[work_index].buyer_matrix_user_id != matrix_user_id {
            return (
                StatusCode::FORBIDDEN,
                Json(json!({ "error": "work order belongs to another buyer", "work_order_id": work_order_id })),
            )
                .into_response();
        }
        if !matches!(
            league.world.world_work_orders[work_index].status.as_str(),
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
                | "cancel_pending_refund"
                | "cancel_pending_chargeback"
        ) {
            let status = league.world.world_work_orders[work_index].status.clone();
            return (
                StatusCode::CONFLICT,
                Json(json!({ "error": "work order is not cancellable before delivery", "status": status, "work_order_id": work_order_id })),
            )
                .into_response();
        }
        let work_order_seed = league.world.world_work_orders[work_index].clone();
        let Some(purchase_index) = indexes
            .purchase_index_by_id
            .get(&work_order_seed.purchase_id)
            .copied()
        else {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "world purchase not found for work order", "work_order_id": work_order_id })),
            )
                .into_response();
        };
        let purchase_seed = league.world.world_purchases[purchase_index].clone();
        let cancellation_refund_scope = latest_world_cancellation_scope_for_work_order(
            &league.world,
            &work_order_seed.work_order_id,
        );
        let retry_chargeback_only = work_order_seed.status == "cancelled_chargeback_failed"
            && world_purchase_buyer_refund_completed(
                &league.world,
                &purchase_seed,
                cancellation_refund_scope.as_deref(),
            );
        let retry_status = if retry_chargeback_only {
            "pending_chargeback"
        } else {
            "pending_refund"
        };
        let cancellation = if matches!(
            work_order_seed.status.as_str(),
            "cancelled_refund_hold"
                | "cancelled_refund_failed"
                | "cancelled_chargeback_failed"
                | "cancel_pending_refund"
                | "cancel_pending_chargeback"
        ) {
            match league
                .world
                .world_work_cancellations
                .iter()
                .rev()
                .find(|cancellation| cancellation.work_order_id == work_order_seed.work_order_id)
                .cloned()
            {
                Some(mut cancellation) => {
                    cancellation.body = body;
                    cancellation.status = retry_status.to_string();
                    cancellation
                }
                None => WorldWorkCancellation {
                    cancellation_id: league_hash_id(
                        "world-cancel",
                        &format!(
                            "{}:{}:{}:{}",
                            work_order_seed.work_order_id,
                            matrix_user_id,
                            world_cancellation_attempt(
                                &league.world,
                                &work_order_seed.work_order_id
                            ),
                            "operation"
                        ),
                    ),
                    work_order_id: work_order_seed.work_order_id.clone(),
                    matrix_user_id: matrix_user_id.clone(),
                    body,
                    status: retry_status.to_string(),
                    refund_status: "pending".to_string(),
                    created_at_epoch: now,
                },
            }
        } else {
            WorldWorkCancellation {
                cancellation_id: league_hash_id(
                    "world-cancel",
                    &format!(
                        "{}:{}:{}:{}",
                        work_order_seed.work_order_id,
                        matrix_user_id,
                        world_cancellation_attempt(&league.world, &work_order_seed.work_order_id),
                        "operation"
                    ),
                ),
                work_order_id: work_order_seed.work_order_id.clone(),
                matrix_user_id: matrix_user_id.clone(),
                body,
                status: retry_status.to_string(),
                refund_status: "pending".to_string(),
                created_at_epoch: now,
            }
        };
        league.world.world_work_orders[work_index].status = if retry_chargeback_only {
            "cancel_pending_chargeback".to_string()
        } else {
            "cancel_pending_refund".to_string()
        };
        if indexes
            .cancellation_index_by_id
            .contains_key(&cancellation.cancellation_id)
        {
            indexes.replace_cancellation_by_id(&mut league.world, &cancellation);
        } else {
            league
                .world
                .world_work_cancellations
                .push(cancellation.clone());
        }
        (
            league.clone(),
            league.world.world_work_orders[work_index].clone(),
            purchase_seed,
            cancellation,
            retry_chargeback_only,
        )
    };
    // Persist the pending cancellation before refund/chargeback network calls so a retry can
    // recover the original cancellation identity and Ledger intent.
    if let Err(response) =
        persist_league_state_after_command(&state, &snapshot.0, "world_work_cancel").await
    {
        return response;
    }
    let buyer_cancel_refund = if snapshot.4 {
        world_replay_buyer_refund_settlement(
            &snapshot.0.world,
            &snapshot.2,
            &snapshot.3.cancellation_id,
        )
        .unwrap_or_else(|| LeagueLedgerSettlement {
            status: "reconcile_required".to_string(),
            error: Some(
                "prior buyer refund lacks an exact typed receipt; seller chargeback is blocked"
                    .to_string(),
            ),
            ..Default::default()
        })
    } else {
        refund_world_purchase_with_ledger(
            &state,
            payload.room_id.as_deref(),
            &snapshot.2,
            &snapshot.3.cancellation_id,
        )
        .await
    };
    let expected_buyer_amount = world_purchase_buyer_amount(&snapshot.2);
    let buyer_refund_amount_matches = expected_buyer_amount
        .is_some_and(|expected| buyer_cancel_refund.amount_credits == Some(expected));
    let buyer_refunded = buyer_cancel_refund
        .progression_allowed_for_term("world_commerce_purchase", &["refunded", "duplicate"])
        && buyer_refund_amount_matches;
    let seller_chargeback = if buyer_refunded {
        chargeback_world_purchase_seller_with_ledger(
            &state,
            payload.room_id.as_deref(),
            &snapshot.2,
            &snapshot.3.cancellation_id,
        )
        .await
    } else {
        LeagueLedgerSettlement {
            status: "skipped_buyer_not_refunded".to_string(),
            error: Some(
                "seller chargeback skipped because buyer refund did not complete".to_string(),
            ),
            ..Default::default()
        }
    };
    let buyer_cancel_refund_receipt = buyer_cancel_refund.term_exchange_receipt.clone();
    let seller_chargeback_receipt = seller_chargeback.term_exchange_receipt.clone();
    let final_snapshot = {
        let mut league = state.inner.league_state.lock().await;
        record_world_term_exchange_receipt(&mut league.world, buyer_cancel_refund_receipt);
        record_world_term_exchange_receipt(&mut league.world, seller_chargeback_receipt);
        let indexes = build_world_indexes(&league.world);
        let mut work_order = snapshot.1.clone();
        let mut purchase = snapshot.2.clone();
        let mut cancellation = snapshot.3.clone();
        let mut economy_event = None;
        let mut standing = None;
        let expected_seller_net_credits =
            world_seller_net_credits_for_price(purchase.price_credits);
        let seller_charged_back = buyer_refunded
            && seller_chargeback.progression_allowed_for_term(
                "world_commerce_purchase",
                &["seller_chargeback_consumed", "duplicate"],
            )
            && seller_chargeback.amount_credits == Some(expected_seller_net_credits);
        let seller_zero_net_skipped = world_settlement_is_zero_seller_net_skip(
            &seller_chargeback,
            expected_seller_net_credits,
        );
        // If no seller settlement ever became active, there is no seller value to claw back.
        // `chargeback_world_purchase_seller_with_ledger` emits this explicit no-op only after
        // checking the immutable settlement evidence; generic skipped outcomes remain holds.
        let seller_not_settled_skipped = buyer_refunded
            && seller_chargeback.status == "skipped_seller_not_settled"
            && seller_chargeback.amount_credits.is_none()
            && seller_chargeback.term_exchange_receipt.is_none();
        let seller_chargeback_cleared = buyer_refunded
            && (seller_charged_back || seller_zero_net_skipped || seller_not_settled_skipped);
        purchase.buyer_consume_status = Some(if buyer_refunded {
            "refunded".to_string()
        } else {
            buyer_cancel_refund.status.clone()
        });
        purchase.buyer_consume_entry_id = buyer_cancel_refund.entry_id.clone();
        purchase.buyer_consume_balance_after = buyer_cancel_refund.balance_after;
        purchase.buyer_consume_error = buyer_cancel_refund.error.clone().or_else(|| {
            (buyer_cancel_refund.progression_allowed_for_term(
                "world_commerce_purchase",
                &["refunded", "duplicate"],
            ) && !buyer_refund_amount_matches)
                .then(|| {
                    format!(
                        "buyer refund amount mismatch: expected {:?}, got {:?}",
                        expected_buyer_amount, buyer_cancel_refund.amount_credits
                    )
                })
        });
        purchase.status = if buyer_refunded {
            if seller_chargeback_cleared {
                "cancelled_refunded".to_string()
            } else {
                "cancelled_chargeback_failed".to_string()
            }
        } else if buyer_cancel_refund.status.starts_with("skipped") {
            "cancelled_refund_hold".to_string()
        } else {
            "cancelled_refund_failed".to_string()
        };
        if buyer_refunded {
            if seller_charged_back || seller_zero_net_skipped || seller_not_settled_skipped {
                purchase.ledger_status = Some(if seller_charged_back {
                    "seller_chargeback_consumed".to_string()
                } else if seller_zero_net_skipped {
                    "skipped_zero_seller_net".to_string()
                } else {
                    "skipped_seller_not_settled".to_string()
                });
                purchase.ledger_entry_id = seller_chargeback
                    .entry_id
                    .clone()
                    .or_else(|| purchase.ledger_entry_id.clone());
                purchase.ledger_balance_after = seller_chargeback
                    .balance_after
                    .or(purchase.ledger_balance_after);
                purchase.ledger_error = seller_chargeback.error.clone();
            }
            if seller_chargeback_cleared {
                let cancelled_event = WorldEconomyEvent {
                    economy_event_id: league_hash_id(
                        "world-econ",
                        &format!(
                            "{}:{}:{}",
                            cancellation.matrix_user_id,
                            cancellation.cancellation_id,
                            cancellation.created_at_epoch
                        ),
                    ),
                    matrix_user_id: cancellation.matrix_user_id.clone(),
                    event_kind: "work_cancelled".to_string(),
                    subject_id: work_order.work_order_id.clone(),
                    credits_delta: purchase.price_credits.saturating_neg(),
                    reputation_delta: 0,
                    created_at_epoch: cancellation.created_at_epoch,
                };
                if push_world_economy_event_once(&mut league.world, cancelled_event.clone()) {
                    standing = Some(upsert_world_faction_standing(
                        &mut league,
                        &cancellation.matrix_user_id,
                        "faction-market-guild",
                        1,
                        cancellation.created_at_epoch,
                    ));
                }
                economy_event = Some(cancelled_event);
            }
            if seller_charged_back {
                let seller_net_credits = seller_chargeback
                    .amount_credits
                    .expect("charged-back seller settlement must carry exact amount_credits");
                let chargeback_event = WorldEconomyEvent {
                    economy_event_id: league_hash_id(
                        "world-seller-chargeback",
                        &format!(
                            "{}:{}:{}",
                            purchase.seller_matrix_user_id,
                            cancellation.cancellation_id,
                            "projection"
                        ),
                    ),
                    matrix_user_id: purchase.seller_matrix_user_id.clone(),
                    event_kind: "seller_chargeback".to_string(),
                    subject_id: purchase.purchase_id.clone(),
                    credits_delta: seller_net_credits.saturating_neg(),
                    reputation_delta: 0,
                    created_at_epoch: cancellation.created_at_epoch,
                };
                if push_world_economy_event_once(&mut league.world, chargeback_event) {
                    if let Some(player) = league
                        .players_by_matrix_user
                        .get_mut(&purchase.seller_matrix_user_id)
                    {
                        if let Some(seller_net_display) =
                            exact_credits_to_legacy_display(seller_net_credits)
                        {
                            // Compatibility projection only: subtract the exact chargeback amount.
                            player.earned_credits =
                                (player.earned_credits - seller_net_display).max(0.0);
                        }
                    }
                }
            }
        }
        work_order.status = purchase.status.clone();
        cancellation.refund_status = buyer_cancel_refund.status.clone();
        cancellation.status = purchase.status.clone();
        indexes.replace_purchase_by_id(&mut league.world, &purchase);
        indexes.replace_work_order_by_id(&mut league.world, &work_order);
        indexes.replace_cancellation_by_id(&mut league.world, &cancellation);
        (
            league.clone(),
            work_order,
            purchase,
            cancellation,
            economy_event,
            standing,
        )
    };
    if let Err(response) =
        persist_league_state_after_command(&state, &final_snapshot.0, "world_work_cancel").await
    {
        return response;
    }
    let WorldRouteArtifacts {
        preview: route_preview,
        task_graph: route_task_graph,
        ..
    } = build_world_route_artifacts(&final_snapshot.0.world);
    (
        StatusCode::OK,
        Json(json!({
            "kind": "trillionnium_world_work_cancellation",
            "world": "trillionnium_world",
            "work_order": final_snapshot.1,
            "purchase": final_snapshot.2,
            "cancellation": final_snapshot.3,
            "economy_event": final_snapshot.4,
            "standing": final_snapshot.5,
            "buyer_cancel_refund_status": buyer_cancel_refund.status,
            "buyer_cancel_refund_entry_id": buyer_cancel_refund.entry_id,
            "buyer_cancel_refund_error": buyer_cancel_refund.error,
            "seller_chargeback_status": seller_chargeback.status,
            "seller_chargeback_entry_id": seller_chargeback.entry_id,
            "seller_chargeback_error": seller_chargeback.error,
            "route_preview": route_preview,
            "route_task_graph": route_task_graph,
        })),
    )
        .into_response()
}

pub(super) async fn cancel_world_work_order(
    Path(work_order_id): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<WorldWorkCancelRequest>,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }
    cancel_world_work_order_inner(state, work_order_id, payload).await
}

pub(super) async fn post_world_web_work_cancel(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(payload): Form<WorldWebWorkCancelRequest>,
) -> Response {
    let web_session = match authorize_league_web_session(&state, &headers, payload.csrf.as_deref())
    {
        Ok(value) => value,
        Err(response) => {
            if matches!(state.config().runtime_profile, RuntimeProfile::LocalDev)
                && cookie_value(&headers, &state.config().league_web_session_cookie_name).is_none()
            {
                None
            } else {
                return response;
            }
        }
    };
    let matrix_user_id = web_session
        .as_ref()
        .map(|session| session.matrix_user_id.clone())
        .or_else(|| {
            normalize_league_matrix_user(
                payload
                    .matrix_user_id
                    .as_deref()
                    .unwrap_or("@alice:local.dev"),
            )
        })
        .unwrap_or_else(|| "@alice:local.dev".to_string());
    let work_order_id = payload
        .work_order_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("latest")
        .to_string();
    let response = cancel_world_work_order_inner(
        state,
        work_order_id,
        WorldWorkCancelRequest {
            matrix_user_id,
            room_id: web_session
                .as_ref()
                .and_then(|session| session.room_id.clone())
                .or_else(|| Some("!web-local:local.dev".to_string())),
            body: payload
                .body
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or("Buyer cancel: record customer deliverable status, evidence package, refund risk controls, next action, and self-review before closing the work order.")
                .to_string(),
        },
    )
    .await;
    if response.status().is_success() {
        Redirect::to("/world?work=cancelled").into_response()
    } else {
        response
    }
}

pub(super) async fn get_world_contracts(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }
    let league = state.inner.league_state.lock().await;
    (
        StatusCode::OK,
        Json(json!({
            "kind": "trillionnium_world_contracts",
            "world": "trillionnium_world",
            "contracts": league.world.world_contracts,
            "completions": league.world.world_contract_completions,
        })),
    )
        .into_response()
}

pub(super) async fn settle_world_contract_completion_with_ledger(
    state: &AppState,
    payload: &WorldContractCompleteRequest,
    matrix_user_id: &str,
    contract: &WorldContract,
    completion: &WorldContractCompletion,
) -> LeagueLedgerSettlement {
    if completion.reward_amount < 0.0 {
        return LeagueLedgerSettlement {
            status: "failed_ledger".to_string(),
            error: Some("world contract reward amount must be non-negative".to_string()),
            ..Default::default()
        };
    }
    if completion.payout_status != "eligible" || !completion.anti_cheat_flags.is_empty() {
        return LeagueLedgerSettlement {
            status: "held_review".to_string(),
            error: Some(format!(
                "world contract payout held: status={} flags={}",
                completion.payout_status,
                completion.anti_cheat_flags.join(",")
            )),
            ..Default::default()
        };
    }
    let (amount_credits, amount_validation_error) =
        match whole_credits_from_compatibility_amount(completion.reward_amount) {
            Ok(value) => (value, None),
            Err(error) => (0, Some(error)),
        };
    CexTermExchangeBackend
        .execute_ledger_action(
            state,
            TermExchangeLedgerActionRequest {
                term_id: "world_contract_completion_settlement".to_string(),
                term_version: "v1".to_string(),
                domain: "trillionnium_world".to_string(),
                intent_id: format!("world_contract_completion:{}", completion.completion_id),
                // CompleteContract is an audit-only, zero-value intent in the native Ledger
                // contract.  This path carries an actual reward amount, so classify it as the
                // value-bearing ReleaseReward operation while retaining the World completion
                // term/intent scope for idempotent evidence.
                intent_kind: term_exchange_protocol::EconomicIntentKind::ReleaseReward,
                room_id: payload.room_id.clone(),
                matrix_user_id: matrix_user_id.to_string(),
                account_id_override: None,
                message: "world contract reward settlement".to_string(),
                failure_context:
                    "matrix identity could not be resolved for world contract settlement"
                        .to_string(),
                ledger_action: "grant".to_string(),
                success_status: "settled".to_string(),
                idempotency_key: format!("world_contract_completion:{}", completion.completion_id),
                idempotency_scope: "world_contract_completion".to_string(),
                reference_id: Some(contract.task_id.clone()),
                amount_credits,
                amount_validation_error,
                currency: "credits".to_string(),
                metadata: json!({
                    "contract_id": contract.contract_id,
                    "completion_id": completion.completion_id,
                    "task_id": contract.task_id,
                    "payout_status": completion.payout_status,
                }),
                extra_ledger_body: Map::new(),
            },
        )
        .await
        .into_legacy_settlement()
}

pub(super) async fn complete_world_contract_inner(
    state: AppState,
    contract_id: String,
    payload: WorldContractCompleteRequest,
) -> Response {
    let matrix_user_id = match normalize_league_matrix_user(&payload.matrix_user_id) {
        Some(value) => value,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "matrix_user_id is required" })),
            )
                .into_response()
        }
    };
    let body = match validate_text_payload(&payload.body, state.config().max_text_chars) {
        Ok(value) => value,
        Err(response) => return response,
    };
    let contract = {
        let league = state.inner.league_state.lock().await;
        let indexes = build_world_indexes(&league.world);
        let Some(contract) = indexes.contract(&league.world, &contract_id).cloned() else {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "world contract not found", "contract_id": contract_id })),
            )
                .into_response();
        };
        let released_completion_exists =
            league
                .world
                .world_contract_completions
                .iter()
                .any(|completion| {
                    completion.contract_id == contract.contract_id
                        && world_contract_completion_released(&league.world, completion)
                });
        // A mutable `completed_*` compatibility status is not proof of a Ledger effect.  Only a
        // typed receipt-backed completion suppresses replay; stale status rows remain retryable.
        if released_completion_exists {
            return (
                StatusCode::CONFLICT,
                Json(json!({
                    "error": "world contract is already completed",
                    "contract_id": contract.contract_id,
                    "status": contract.status,
                    "cex_status": contract.cex_status,
                })),
            )
                .into_response();
        }
        contract
    };
    if contract.actor_matrix_user_id != matrix_user_id {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({
                "error": "world contract can only be completed by its creator",
                "contract_id": contract.contract_id,
            })),
        )
            .into_response();
    }
    let judgement = judge_league_submission_with_pipeline(&state, &body, "world_contract").await;
    let mut completion = {
        let now = Utc::now().timestamp();
        let mut league = state.inner.league_state.lock().await;
        let indexes = build_world_indexes(&league.world);
        // Completion identity is derived from the immutable contract + actor + report body.  A
        // retry after a lost response therefore reuses the same Ledger intent; a distinct report
        // is a distinct attempted completion and cannot overwrite the prior evidence.
        let deterministic_completion_id = league_hash_id(
            "world-contract-completion",
            &format!("{}:{}:{}", contract.contract_id, matrix_user_id, body),
        );
        // Tactics and the direct contract endpoint historically used different completion
        // namespaces.  A tactics response can be lost after the row is durable, then recovered
        // through this endpoint; derive/check both current ids before falling back to timestamp
        // identities.  Every candidate is still bound to the immutable contract/actor/body tuple
        // so an id collision fails closed rather than creating a second Ledger intent.
        let tactics_completion_id = league_hash_id(
            "world-trillionnium-task-completion",
            &format!("{}:{}:{}", contract.contract_id, matrix_user_id, body),
        );
        let mut existing_completion_by_id = None;
        for candidate_id in [&deterministic_completion_id, &tactics_completion_id] {
            if let Some(existing_completion) = league
                .world
                .world_contract_completions
                .iter()
                .find(|existing| existing.completion_id == *candidate_id)
                .cloned()
            {
                if !world_contract_completion_identity_matches(
                    &existing_completion,
                    &contract.contract_id,
                    &matrix_user_id,
                    &body,
                ) {
                    return world_contract_completion_id_collision_response(
                        candidate_id,
                        "world contract completion_id is already bound to a different payload",
                    );
                }
                // If both namespaces exist (for example, a concurrent upgrade straddled the
                // route cutover), prefer the terminal/receipt-backed row; otherwise retain the
                // first canonical HTTP candidate deterministically.
                let should_replace = existing_completion_by_id.as_ref().is_some_and(|current| {
                    world_contract_completion_replay_priority(&league.world, &existing_completion)
                        > world_contract_completion_replay_priority(&league.world, current)
                });
                if existing_completion_by_id.is_none() || should_replace {
                    existing_completion_by_id = Some(existing_completion);
                }
            }
        }
        let existing_completion = existing_completion_by_id.or_else(|| {
            // Timestamp-derived rows from either historical route are valid recovery candidates
            // here: this endpoint is the documented settlement authority for both surfaces.
            legacy_world_contract_completion_for_request(
                &league.world,
                &contract.contract_id,
                &matrix_user_id,
                &body,
            )
        });
        let completion = existing_completion.unwrap_or_else(|| WorldContractCompletion {
            completion_id: deterministic_completion_id.clone(),
            contract_id: contract.contract_id.clone(),
            matrix_user_id: matrix_user_id.clone(),
            body: body.clone(),
            score: judgement.score,
            grade: judgement.grade.clone(),
            reward_amount: judgement.reward_amount,
            judge_status: judgement.judge_status.clone(),
            payout_status: judgement.payout_status.clone(),
            anti_cheat_flags: judgement.anti_cheat_flags.clone(),
            score_events: judgement.score_events.clone(),
            ledger_status: Some("pending".to_string()),
            ledger_account_id: None,
            ledger_entry_id: None,
            ledger_balance_after: None,
            ledger_error: None,
            created_at_epoch: now,
        });
        let released = completion.payout_status == "eligible";
        if let Some(contract_index) = indexes.contract_index(&contract.contract_id) {
            let terminal_projection_marker = matches!(
                completion.ledger_status.as_deref(),
                Some("settled")
                    | Some("approved_release")
                    | Some("duplicate")
                    | Some("skipped_zero_reward")
            );
            // Evaluate the receipt predicate before taking a mutable borrow of the contract.  The
            // world lock is already held, and keeping this immutable check separate avoids
            // aliasing the same `league.world` while projecting the compatibility row.
            let exact_receipt_present =
                world_contract_completion_released(&league.world, &completion);
            let stored_contract = &mut league.world.world_contracts[contract_index];
            if terminal_projection_marker && !exact_receipt_present {
                // A legacy row can claim a terminal compatibility status while its exact receipt
                // is absent or malformed.  Preserve the fact that this is a reconciliation case
                // instead of regressing it to a fresh pending/review projection; the settlement
                // call below still uses the original completion id and remains fail-closed.
                stored_contract.status = "settlement_reconcile_required".to_string();
                stored_contract.cex_status = Some("reconcile_required".to_string());
            } else {
                stored_contract.status = if released {
                    "completed_pending_settlement".to_string()
                } else {
                    "review_hold".to_string()
                };
                stored_contract.cex_status = Some(if released {
                    "settlement_pending".to_string()
                } else {
                    "review_hold".to_string()
                });
            }
        }
        if !league
            .world
            .world_contract_completions
            .iter()
            .any(|existing| existing.completion_id == completion.completion_id)
        {
            league
                .world
                .world_contract_completions
                .push(completion.clone());
        }
        completion
    };
    // Completion identity/status is now durable before the remote settlement.  If the process
    // dies after Ledger commits but before the response is returned, the next request can replay
    // the same completion_id and exact Ledger intent.
    let pending_snapshot = {
        let league = state.inner.league_state.lock().await;
        league.clone()
    };
    if let Err(response) =
        persist_league_state_after_command(&state, &pending_snapshot, "world_contract_completion")
            .await
    {
        return response;
    }
    let settlement = settle_world_contract_completion_with_ledger(
        &state,
        &payload,
        &matrix_user_id,
        &contract,
        &completion,
    )
    .await;
    let exact_reward_credits = whole_credits_from_compatibility_amount(completion.reward_amount);
    let expected_reward_credits = exact_reward_credits.as_ref().ok().copied();
    let settlement_progression_allowed = settlement.progression_allowed_for_term(
        "world_contract_completion_settlement",
        &["settled", "duplicate"],
    );
    let settlement_amount_matches =
        expected_reward_credits.is_some_and(|expected| settlement.amount_credits == Some(expected));
    let settlement_completed = settlement_progression_allowed && settlement_amount_matches;
    let settlement_zero_reward_skipped =
        world_settlement_is_zero_reward_skip(&settlement, expected_reward_credits);
    completion.ledger_status = Some(settlement.status.clone());
    completion.ledger_account_id = settlement.account_id.clone();
    completion.ledger_entry_id = settlement.entry_id.clone();
    completion.ledger_balance_after = settlement.balance_after;
    completion.ledger_error = settlement.error.clone().or_else(|| {
        (settlement_progression_allowed && !settlement_amount_matches).then(|| {
            format!(
                "world contract settlement amount mismatch: expected {:?}, got {:?}",
                expected_reward_credits, settlement.amount_credits
            )
        })
    });
    let snapshot = {
        let mut league = state.inner.league_state.lock().await;
        completion = merge_world_contract_completion_settlement(
            &mut league,
            &contract,
            &completion,
            &settlement,
            settlement_completed,
            settlement_zero_reward_skipped,
        );
        league.clone()
    };
    if let Err(response) =
        persist_league_state_after_command(&state, &snapshot, "world_contract_completion").await
    {
        return response;
    }
    let response_contract = {
        let indexes = build_world_indexes(&snapshot.world);
        indexes
            .contract(&snapshot.world, &contract.contract_id)
            .cloned()
    };
    (
        StatusCode::OK,
        Json(json!({
            "kind": "trillionnium_world_contract_completion",
            "world": "trillionnium_world",
            "contract": response_contract,
            "completion": completion,
        })),
    )
        .into_response()
}

pub(super) async fn complete_world_contract(
    Path(contract_id): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<WorldContractCompleteRequest>,
) -> Response {
    if let Err(response) = authorize_ingress(&headers, state.config()) {
        state.inner.metrics.inc_ingress_auth_failures();
        return response;
    }
    complete_world_contract_inner(state, contract_id, payload).await
}

pub(super) async fn post_world_web_contract_complete(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(payload): Form<WorldWebContractCompleteRequest>,
) -> Response {
    let web_session = match authorize_league_web_session(&state, &headers, payload.csrf.as_deref())
    {
        Ok(value) => value,
        Err(response) => {
            if matches!(state.config().runtime_profile, RuntimeProfile::LocalDev)
                && cookie_value(&headers, &state.config().league_web_session_cookie_name).is_none()
            {
                None
            } else {
                return response;
            }
        }
    };
    let matrix_user_id = web_session
        .as_ref()
        .map(|session| session.matrix_user_id.clone())
        .or_else(|| {
            normalize_league_matrix_user(
                payload
                    .matrix_user_id
                    .as_deref()
                    .unwrap_or("@alice:local.dev"),
            )
        })
        .unwrap_or_else(|| "@alice:local.dev".to_string());
    let contract_id = match payload
        .contract_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
    {
        Some(value) => value,
        None => {
            let league = state.inner.league_state.lock().await;
            let indexes = build_world_indexes(&league.world);
            match indexes
                .latest_contract_index_for_actor(&matrix_user_id)
                .and_then(|index| league.world.world_contracts.get(index))
                .map(|contract| contract.contract_id.clone())
            {
                Some(value) => value,
                None => return Redirect::to("/world?contract=missing").into_response(),
            }
        }
    };
    let body = payload
        .body
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("World contract report: customer deliverable, evidence package, risk review, next step, rating standards, and self-review.")
        .to_string();
    let request = WorldContractCompleteRequest {
        matrix_user_id,
        room_id: web_session
            .as_ref()
            .and_then(|session| session.room_id.clone())
            .or_else(|| Some("!web-local:local.dev".to_string())),
        body,
    };
    let response = complete_world_contract_inner(state, contract_id, request).await;
    if response.status().is_success() {
        Redirect::to("/world?contract=completed").into_response()
    } else {
        response
    }
}
