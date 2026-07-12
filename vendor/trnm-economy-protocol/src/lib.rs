use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub const TERM_EXCHANGE_PROTOCOL_VERSION: &str = "term_exchange_protocol_v2";
pub const TERM_EXCHANGE_PROTOCOL_PACKAGE_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const TERM_EXCHANGE_KERNEL_CONTRACT_VERSION: &str = "trillionnium_term_exchange_kernel_v2";
pub const TERM_EXCHANGE_BACKEND_CONTRACT_VERSION: &str = "term_exchange_backend_v2";
pub const TERM_EXCHANGE_KERNEL_ID: &str = "term-exchange-kernel";
pub const CEX_SETTLEMENT_BACKEND_ID: &str = "cex-settlement-backend";
pub const CEX_SETTLEMENT_BACKEND_NAME: &str = "CEX Settlement Backend";
pub const OFFLINE_LOCAL_BACKEND_ID: &str = "trnm-offline-local-backend";
pub const LEGACY_CEX_RUNTIME_PLUGIN_CONTRACT_VERSION: &str = "trillionnium_cex_runtime_plugin_v1";
pub const TRNM_ECONOMY_POLICY_VERSION: &str = "trnm_economy_policy_v1";
pub const BATTLE_WALLET_REWARD_PER_EVENT_CAP: i64 = 100;
pub const BATTLE_WALLET_REWARD_DAILY_CAP: i64 = 300;
pub const SELLER_REVERSIBLE_WINDOW_SECONDS: i64 = 86_400;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TrnmEconomyPolicy {
    pub policy_version: String,
    pub local_soft_credit_convertible_to_wallet: bool,
    pub battle_wallet_reward_per_event_cap: i64,
    pub battle_wallet_reward_daily_cap: i64,
    pub seller_reversible_window_seconds: i64,
    pub public_player_market_enabled: bool,
}

impl Default for TrnmEconomyPolicy {
    fn default() -> Self {
        Self {
            policy_version: TRNM_ECONOMY_POLICY_VERSION.to_string(),
            local_soft_credit_convertible_to_wallet: false,
            battle_wallet_reward_per_event_cap: BATTLE_WALLET_REWARD_PER_EVENT_CAP,
            battle_wallet_reward_daily_cap: BATTLE_WALLET_REWARD_DAILY_CAP,
            seller_reversible_window_seconds: SELLER_REVERSIBLE_WINDOW_SECONDS,
            public_player_market_enabled: false,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EconomyMode {
    #[default]
    OfflineLocal,
    CexConnected,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EconomyCurrencyClass {
    SoftCredits,
    WalletCredits,
    TemporaryBattleResource,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EconomyAssetClass {
    BoundGameplayItem,
    TradeableItem,
    WalletCredit,
    TemporaryBattleResource,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EconomyTransferability {
    Bound,
    Tradeable,
    Ephemeral,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct EconomyAssetSemantic {
    pub asset_id: String,
    pub asset_class: EconomyAssetClass,
    pub transferability: EconomyTransferability,
    pub settlement_authority: String,
}

impl EconomyAssetSemantic {
    pub fn soft_credit() -> Self {
        Self {
            asset_id: "trnm-soft-credit".to_string(),
            asset_class: EconomyAssetClass::BoundGameplayItem,
            transferability: EconomyTransferability::Bound,
            settlement_authority: "trnm-campaign-core".to_string(),
        }
    }

    pub fn wallet_credit() -> Self {
        Self {
            asset_id: "cex-wallet-credit".to_string(),
            asset_class: EconomyAssetClass::WalletCredit,
            transferability: EconomyTransferability::Tradeable,
            settlement_authority: CEX_SETTLEMENT_BACKEND_ID.to_string(),
        }
    }

    pub fn temporary_battle_resource(resource_id: impl Into<String>) -> Self {
        Self {
            asset_id: resource_id.into(),
            asset_class: EconomyAssetClass::TemporaryBattleResource,
            transferability: EconomyTransferability::Ephemeral,
            settlement_authority: "trnm-rts-sim".to_string(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct EconomyAccountBinding {
    pub actor_id: String,
    pub account_id: String,
    pub binding_revision: u64,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct WalletSnapshot {
    pub account_id: String,
    pub available_credits: i64,
    pub reserved_credits: i64,
    pub observed_at_cursor: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
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

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
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

    pub fn allows_progression(&self) -> bool {
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

impl EconomicIntent {
    pub fn validate(&self) -> Result<(), String> {
        if self.protocol_version != TERM_EXCHANGE_PROTOCOL_VERSION
            || self.intent_id.trim().is_empty()
            || self.term_id.trim().is_empty()
            || self.domain != "trnm_game"
            || self.idempotency_key.scope.trim().is_empty()
            || self.idempotency_key.key.trim().is_empty()
            || self.actors.is_empty()
            || self
                .actors
                .iter()
                .any(|actor| actor.actor_id.trim().is_empty())
        {
            return Err("economic intent violates the TRNM protocol boundary".to_string());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
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

    pub fn from_intent(
        receipt_id: impl Into<String>,
        intent: &EconomicIntent,
        backend_id: impl Into<String>,
        backend_kind: SettlementBackendKind,
        status: ReceiptStatus,
        finalized_at_epoch: i64,
    ) -> Self {
        Self::new(
            receipt_id,
            intent.intent_id.clone(),
            intent.term_id.clone(),
            backend_id,
            backend_kind,
            status,
            finalized_at_epoch,
        )
    }

    pub fn validate_for(&self, intent: &EconomicIntent) -> Result<(), String> {
        if self.protocol_version != TERM_EXCHANGE_PROTOCOL_VERSION
            || self.intent_id != intent.intent_id
            || self.term_id != intent.term_id
            || self.progression_class != self.status.progression_class()
            || self.receipt_id.trim().is_empty()
        {
            return Err("economic receipt is not bound to the pending intent".to_string());
        }
        Ok(())
    }

    pub fn allows_progression(&self) -> bool {
        self.status.allows_progression()
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
        capabilities: [
            "wallet_read_model",
            "reserve",
            "seller_settlement",
            "buyer_consume",
            "refund",
            "seller_chargeback",
            "reward_release",
            "audit_receipts",
            "reconciliation",
        ]
        .into_iter()
        .map(str::to_string)
        .collect(),
        receipt_verification_required: true,
    }
}

pub fn protocol_manifest_json() -> Value {
    json!({
        "protocol_version": TERM_EXCHANGE_PROTOCOL_VERSION,
        "package_version": TERM_EXCHANGE_PROTOCOL_PACKAGE_VERSION,
        "kernel_contract_version": TERM_EXCHANGE_KERNEL_CONTRACT_VERSION,
        "backend_contract_version": TERM_EXCHANGE_BACKEND_CONTRACT_VERSION,
        "domain": "trnm_game",
        "core_types": ["EconomicIntent", "EconomicReceipt", "ReceiptStatus", "ReceiptProgressionClass", "SettlementBackendManifest", "IdempotencyKey", "ActorRef", "AssetRef"],
        "asset_classes": ["bound_gameplay_item", "tradeable_item", "wallet_credit", "temporary_battle_resource"],
        "currency_classes": ["soft_credits", "wallet_credits", "temporary_battle_resource"],
        "economy_policy": TrnmEconomyPolicy::default(),
        "world_progression_rule": "TRNM applies tradeable value only after a verified receipt allows progression; recoverable holds remain in the durable outbox."
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn currency_and_asset_authorities_are_explicit() {
        assert_eq!(
            EconomyAssetSemantic::soft_credit().settlement_authority,
            "trnm-campaign-core"
        );
        assert_eq!(
            EconomyAssetSemantic::wallet_credit().settlement_authority,
            CEX_SETTLEMENT_BACKEND_ID
        );
        assert_eq!(
            EconomyAssetSemantic::temporary_battle_resource("cyan").transferability,
            EconomyTransferability::Ephemeral
        );
    }

    #[test]
    fn receipt_statuses_fail_closed() {
        assert!(ReceiptStatus::Settled.allows_progression());
        assert!(!ReceiptStatus::FailedNetwork.allows_progression());
        assert!(!ReceiptStatus::FailedBadResponse.allows_progression());
    }

    #[test]
    fn monetary_policy_forbids_soft_to_wallet_conversion_and_bounds_rewards() {
        let policy = TrnmEconomyPolicy::default();
        assert!(!policy.local_soft_credit_convertible_to_wallet);
        assert_eq!(policy.battle_wallet_reward_per_event_cap, 100);
        assert_eq!(policy.battle_wallet_reward_daily_cap, 300);
        assert_eq!(policy.seller_reversible_window_seconds, 86_400);
        assert!(!policy.public_player_market_enabled);
    }
}
