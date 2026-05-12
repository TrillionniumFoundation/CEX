use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub const TERM_EXCHANGE_PROTOCOL_VERSION: &str = "term_exchange_protocol_v1";
pub const TERM_EXCHANGE_KERNEL_CONTRACT_VERSION: &str = "trillionnium_term_exchange_kernel_v1";
pub const TERM_EXCHANGE_BACKEND_CONTRACT_VERSION: &str = "term_exchange_backend_v1";
pub const TERM_EXCHANGE_KERNEL_ID: &str = "term-exchange-kernel";
pub const CEX_SETTLEMENT_BACKEND_ID: &str = "cex-settlement-backend";
pub const CEX_SETTLEMENT_BACKEND_NAME: &str = "CEX Settlement Backend";
pub const LEGACY_CEX_RUNTIME_PLUGIN_CONTRACT_VERSION: &str = "trillionnium_cex_runtime_plugin_v1";

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SettlementBackendKind {
    Cex,
    Dex,
    Chain,
    LocalTest,
    External,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EconomicIntentKind {
    Reserve,
    Settle,
    Consume,
    Refund,
    Chargeback,
    ReleaseReward,
    CompleteContract,
    Quote,
    VerifyReceipt,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReceiptProgressionClass {
    ProgressionAllowed,
    RecoverableHold,
    TerminalSkip,
    HardFail,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReceiptStatus {
    Reserved,
    Settled,
    Consumed,
    Refunded,
    SellerChargebackReserved,
    SellerChargebackConsumed,
    ApprovedRelease,
    Duplicate,
    HeldReview,
    SkippedZeroPrice,
    SkippedZeroReward,
    SkippedZeroSellerNet,
    SkippedMissingRoom,
    SkippedMissingAccount,
    SkippedMissingLedgerToken,
    FailedNetwork,
    FailedIdentity,
    FailedLedger,
    FailedBadResponse,
    MissingAccount,
    MissingLedgerToken,
    SellerChargebackReserveFailed,
    SellerChargebackFailed,
    RejectedRefundFailed,
    CancelledRefundFailed,
}

impl ReceiptStatus {
    pub fn progression_class(&self) -> ReceiptProgressionClass {
        match self {
            Self::Reserved
            | Self::Settled
            | Self::Consumed
            | Self::Refunded
            | Self::SellerChargebackReserved
            | Self::SellerChargebackConsumed
            | Self::ApprovedRelease
            | Self::Duplicate => ReceiptProgressionClass::ProgressionAllowed,
            Self::SkippedZeroPrice | Self::SkippedZeroReward | Self::SkippedZeroSellerNet => {
                ReceiptProgressionClass::TerminalSkip
            }
            Self::HeldReview
            | Self::SkippedMissingRoom
            | Self::FailedNetwork
            | Self::FailedIdentity
            | Self::FailedLedger
            | Self::SellerChargebackReserveFailed
            | Self::SellerChargebackFailed
            | Self::RejectedRefundFailed
            | Self::CancelledRefundFailed => ReceiptProgressionClass::RecoverableHold,
            Self::FailedBadResponse
            | Self::SkippedMissingAccount
            | Self::SkippedMissingLedgerToken
            | Self::MissingAccount
            | Self::MissingLedgerToken => ReceiptProgressionClass::HardFail,
        }
    }

    pub fn allows_world_progression(&self) -> bool {
        matches!(
            self.progression_class(),
            ReceiptProgressionClass::ProgressionAllowed | ReceiptProgressionClass::TerminalSkip
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct IdempotencyKey {
    pub scope: String,
    pub key: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ActorRef {
    pub actor_id: String,
    pub actor_kind: String,
    pub account_id: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AssetRef {
    pub asset_id: String,
    pub asset_kind: String,
    pub quantity: i64,
    pub unit: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TermDefinition {
    pub protocol_version: String,
    pub term_id: String,
    pub term_version: String,
    pub domain: String,
    pub authority: String,
    pub pricing_rule_id: String,
    pub settlement_rule_id: String,
    pub refund_rule_id: Option<String>,
    pub dispute_rule_id: Option<String>,
    pub receipt_schema_id: String,
    pub projection_schema_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EconomicIntent {
    pub protocol_version: String,
    pub intent_id: String,
    pub term_id: String,
    pub term_version: String,
    pub domain: String,
    pub kind: EconomicIntentKind,
    pub idempotency_key: IdempotencyKey,
    pub actors: Vec<ActorRef>,
    pub assets: Vec<AssetRef>,
    pub amount_credits: Option<i64>,
    pub currency: Option<String>,
    pub metadata: Value,
    pub created_at_epoch: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EconomicReceipt {
    pub protocol_version: String,
    pub receipt_id: String,
    pub intent_id: String,
    pub term_id: String,
    pub backend_id: String,
    pub backend_kind: SettlementBackendKind,
    pub status: ReceiptStatus,
    pub progression_class: ReceiptProgressionClass,
    pub settlement_reference: Option<String>,
    pub ledger_entry_id: Option<String>,
    pub reason: Option<String>,
    pub evidence: Value,
    pub finalized_at_epoch: i64,
}

impl EconomicReceipt {
    pub fn new(
        receipt_id: impl Into<String>,
        intent_id: impl Into<String>,
        term_id: impl Into<String>,
        backend_id: impl Into<String>,
        backend_kind: SettlementBackendKind,
        status: ReceiptStatus,
        finalized_at_epoch: i64,
    ) -> Self {
        Self {
            protocol_version: TERM_EXCHANGE_PROTOCOL_VERSION.to_string(),
            receipt_id: receipt_id.into(),
            intent_id: intent_id.into(),
            term_id: term_id.into(),
            backend_id: backend_id.into(),
            backend_kind,
            progression_class: status.progression_class(),
            status,
            settlement_reference: None,
            ledger_entry_id: None,
            reason: None,
            evidence: json!({}),
            finalized_at_epoch,
        }
    }

    pub fn allows_world_progression(&self) -> bool {
        self.status.allows_world_progression()
            && self.progression_class == self.status.progression_class()
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SettlementBackendManifest {
    pub backend_id: String,
    pub backend_name: String,
    pub backend_kind: SettlementBackendKind,
    pub contract_version: String,
    pub active: bool,
    pub fail_closed: bool,
    pub capabilities: Vec<String>,
    pub receipt_verification_required: bool,
}

pub fn cex_settlement_backend_manifest(active: bool) -> SettlementBackendManifest {
    SettlementBackendManifest {
        backend_id: CEX_SETTLEMENT_BACKEND_ID.to_string(),
        backend_name: CEX_SETTLEMENT_BACKEND_NAME.to_string(),
        backend_kind: SettlementBackendKind::Cex,
        contract_version: TERM_EXCHANGE_BACKEND_CONTRACT_VERSION.to_string(),
        active,
        fail_closed: true,
        capabilities: vec![
            "wallet_read_model".to_string(),
            "reserve".to_string(),
            "seller_settlement".to_string(),
            "buyer_consume".to_string(),
            "refund".to_string(),
            "seller_chargeback".to_string(),
            "reward_release".to_string(),
            "review_hold_release".to_string(),
            "work_order_economy".to_string(),
            "audit_receipts".to_string(),
            "recovery_dead_letter".to_string(),
        ],
        receipt_verification_required: true,
    }
}

pub fn protocol_manifest_json() -> Value {
    json!({
        "protocol_version": TERM_EXCHANGE_PROTOCOL_VERSION,
        "kernel_contract_version": TERM_EXCHANGE_KERNEL_CONTRACT_VERSION,
        "backend_contract_version": TERM_EXCHANGE_BACKEND_CONTRACT_VERSION,
        "core_types": [
            "TermDefinition",
            "EconomicIntent",
            "EconomicReceipt",
            "ReceiptStatus",
            "ReceiptProgressionClass",
            "SettlementBackendManifest",
            "IdempotencyKey",
            "ActorRef",
            "AssetRef"
        ],
        "backend_kinds": ["cex", "dex", "chain", "local_test", "external"],
        "world_progression_rule": "World/domain runtimes advance economic state only after EconomicReceipt allows progression or terminal skip; recoverable holds route to retry/recovery."
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn receipt_status_maps_to_progression_class() {
        assert_eq!(
            ReceiptStatus::Settled.progression_class(),
            ReceiptProgressionClass::ProgressionAllowed
        );
        assert_eq!(
            ReceiptStatus::SellerChargebackFailed.progression_class(),
            ReceiptProgressionClass::RecoverableHold
        );
        assert_eq!(
            ReceiptStatus::Duplicate.progression_class(),
            ReceiptProgressionClass::ProgressionAllowed
        );
        assert_eq!(
            ReceiptStatus::HeldReview.progression_class(),
            ReceiptProgressionClass::RecoverableHold
        );
        assert_eq!(
            ReceiptStatus::SkippedZeroSellerNet.progression_class(),
            ReceiptProgressionClass::TerminalSkip
        );
        assert_eq!(
            ReceiptStatus::MissingLedgerToken.progression_class(),
            ReceiptProgressionClass::HardFail
        );
    }

    #[test]
    fn cex_backend_is_first_fail_closed_backend() {
        let manifest = cex_settlement_backend_manifest(true);
        assert_eq!(manifest.backend_id, CEX_SETTLEMENT_BACKEND_ID);
        assert_eq!(manifest.backend_kind, SettlementBackendKind::Cex);
        assert!(manifest.active);
        assert!(manifest.fail_closed);
        assert!(manifest
            .capabilities
            .iter()
            .any(|capability| capability == "seller_chargeback"));
    }
}
