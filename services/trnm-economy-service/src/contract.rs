use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

pub const TERM_EXCHANGE_PROTOCOL_VERSION: &str = "term_exchange_protocol_v2";
pub const SETTLEMENT_CONTRACT_VERSION: &str = "trnm_cex_settlement_v1";
pub const SETTLEMENT_RECEIPT_LOOKUP_CONTRACT: &str =
    "trnm_cex_settlement_receipt_lookup_v1";
pub const SETTLEMENT_ERROR_CONTRACT: &str = "trnm_cex_settlement_error_v1";
pub const SETTLEMENT_BACKEND_ID: &str = "cex-settlement-backend";
pub const GAME_AUTHORITY_HEADER: &str = "x-trnm-game-authority";
pub const INTENT_HASH_HEADER: &str = "x-trnm-intent-sha256";
pub const EXPECTED_GAME_AUTHORITY_AUDIENCE: &str = "trnm-cex-settlement-v1";
pub const SERVER_SIGNED_VALUE_ENTITLEMENT_V2_CONTRACT: &str =
    "trnm_server_signed_value_entitlement_v2";
pub const SERVER_SIGNED_VALUE_ENTITLEMENT_METADATA_KEY: &str =
    "server_signed_value_entitlement";
pub const ENTITLEMENT_SIGNER_ISSUER: &str = "trnm-online-game-server";
pub const BATTLE_WALLET_REWARD_PER_EVENT_CAP: i64 = 100;
pub const BATTLE_WALLET_REWARD_DAILY_CAP: i64 = 300;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct IdempotencyKey {
    pub scope: String,
    pub key: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ActorRef {
    pub actor_id: String,
    pub actor_kind: String,
    pub account_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AssetRef {
    pub asset_id: String,
    pub asset_kind: String,
    pub quantity: i64,
    pub unit: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SettlementBackendKind {
    Cex,
    Dex,
    Chain,
    LocalTest,
    External,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReceiptProgressionClass {
    ProgressionAllowed,
    RecoverableHold,
    TerminalSkip,
    HardFail,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
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
    pub fn progression_class(self) -> ReceiptProgressionClass {
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValueEntitlementSource {
    Battle,
    Contract,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ServerSignedValueEntitlementV2 {
    pub contract_version: String,
    pub entitlement_id: String,
    pub issuer: String,
    pub key_id: String,
    pub signature_algorithm: String,
    pub actor_id: String,
    pub account_id: String,
    pub source: ValueEntitlementSource,
    pub source_id: String,
    pub intent_id: String,
    pub amount_credits: i64,
    pub currency: String,
    pub budget_day: u32,
    pub issued_at_epoch: i64,
    pub expires_at_epoch: i64,
    pub match_id: String,
    pub rules_version: String,
    pub build_id: String,
    pub result_hash: String,
    pub participants_hash: String,
    pub nonce: String,
    pub signature: String,
}

impl ServerSignedValueEntitlementV2 {
    pub fn signing_payload(&self) -> Result<Vec<u8>, String> {
        let mut unsigned = self.clone();
        unsigned.signature.clear();
        serde_json::to_vec(&unsigned)
            .map_err(|error| format!("encode v2 value entitlement failed: {error}"))
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct SubmitIntentRequest {
    pub intent: EconomicIntent,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct SettlementReceiptLookupResponse {
    pub contract_version: String,
    pub intent_id: String,
    pub intent_hash: String,
    pub receipt: EconomicReceipt,
}

#[derive(Clone, Debug, Deserialize)]
pub struct EntitlementIssuerKeyStatusRequest {
    pub key_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct EntitlementIssuerKeyStatusResponse {
    pub key_id: String,
    pub issuer: String,
    pub status: String,
    pub signature_algorithm: String,
    pub public_key_sha256: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct SettlementReadiness {
    pub status: String,
    pub contract_version: String,
    pub postgres: bool,
    pub receipt_lookup: bool,
    pub authority_principals: usize,
    pub active_issuer_keys: usize,
    pub public_player_market_enabled: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct SettlementErrorBody {
    pub contract_version: String,
    pub code: String,
    pub message: String,
    pub retryable: bool,
}

pub fn canonical_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

pub fn serialized_intent_hash(intent: &EconomicIntent) -> Result<String, String> {
    let encoded = serde_json::to_vec(intent)
        .map_err(|error| format!("encode economic intent for hashing: {error}"))?;
    Ok(format!("{:x}", Sha256::digest(encoded)))
}

pub fn stable_receipt_id(intent_hash: &str) -> String {
    format!("trnm-cex-receipt-v1:{intent_hash}")
}

pub fn stable_settlement_reference(intent_hash: &str) -> String {
    format!("trnm-cex-settlement-v1:{intent_hash}")
}
