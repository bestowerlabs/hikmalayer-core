use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::Json,
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::{
    auth::AuthManager,
    blockchain::{
        block::Block,
        chain::Blockchain,
        state::ChainState,
        transaction::{CredentialAction, SlashProof, Transaction, TransactionType},
    },
    consensus::{pos, pow, vrf},
    contract::contract::ContractExecutor,
    governance::GovernanceConfig,
    p2p::{
        protocol::{P2PEnvelope, P2PPayload, SeenMessageCache},
        service::P2PService,
    },
    persistence::{save_state, AppSnapshot},
};

/// This node's own validator identity, loaded from the local environment.
/// The key never leaves the node and is never accepted over the network.
#[derive(Clone)]
pub struct LocalValidatorKey {
    pub address: String,
    pub public_key: String,
    pub vrf_public_key: String,
    pub private_key: String,
}

impl LocalValidatorKey {
    pub fn from_private_key(private_key_hex: &str) -> Result<Self, String> {
        let public_key = pos::derive_public_key(private_key_hex)?;
        let address = pos::derive_address(&public_key)?;
        let vrf_public_key = vrf::derive_vrf_public_key(private_key_hex)?;
        Ok(Self {
            address,
            public_key,
            vrf_public_key,
            private_key: private_key_hex.to_string(),
        })
    }
}

#[derive(Clone)]
pub struct AppState {
    pub chain: Arc<Mutex<Blockchain>>,
    pub contracts: Arc<Mutex<ContractExecutor>>,
    pub pending_transactions: Arc<Mutex<Vec<Transaction>>>,
    pub auth_manager: Arc<Mutex<AuthManager>>,
    pub peers: Arc<Mutex<Vec<String>>>,
    pub governance: Arc<Mutex<GovernanceConfig>>,
    pub slash_evidence: Arc<Mutex<Vec<crate::persistence::SlashEvidence>>>,
    pub metrics: Arc<Mutex<Metrics>>,
    pub seen_messages: Arc<Mutex<SeenMessageCache>>,
    /// Accepted bearer tokens (first = current; extras support rotation).
    pub p2p_tokens: Vec<String>,
    pub admin_tokens: Vec<String>,
    pub p2p_service: Arc<P2PService>,
    pub validator_key: Option<LocalValidatorKey>,
    /// Treasury key for dev/test faucet operation (never required in
    /// production; the faucet is disabled without it).
    pub treasury_key: Option<LocalValidatorKey>,
}

#[derive(Deserialize)]
pub struct CertificateRequest {
    pub id: String,
    pub issued_to: String,
    pub description: String,
}

#[derive(Deserialize)]
pub struct VerifyCertificateRequest {
    pub id: String,
}

#[derive(Deserialize)]
pub struct TokenTransferRequest {
    pub from: String,
    pub to: String,
    pub amount: u64,
    #[serde(default)]
    pub nonce: u64,
    pub public_key: Option<String>,
    pub signature: Option<String>,
}

#[derive(Deserialize)]
pub struct FaucetRequest {
    pub to: String,
    pub amount: u64,
}

#[derive(Deserialize)]
pub struct CredentialWriteRequest {
    pub id: String,
    #[serde(default)]
    pub subject: String,
    #[serde(default)]
    pub data_hash: String,
    #[serde(default)]
    pub revoke: bool,
    pub issuer: String,
    #[serde(default)]
    pub nonce: u64,
    pub public_key: Option<String>,
    pub signature: Option<String>,
}

#[derive(Deserialize)]
pub struct DifficultyRequest {
    pub difficulty: usize,
}

#[derive(Deserialize)]
pub struct StakeRequest {
    pub address: String,
    pub amount: u64,
    pub public_key: Option<String>,
    pub vrf_public_key: Option<String>,
    #[serde(default)]
    pub nonce: u64,
    pub signature: Option<String>,
}

#[derive(Deserialize)]
pub struct PeerRequest {
    pub address: String,
}

#[derive(Deserialize)]
pub struct GovernanceRequest {
    pub slash_percent: u64,
    pub finality_depth: u64,
}

#[derive(Deserialize)]
pub struct SlashEvidenceRequest {
    pub block_index: u64,
    pub reporter: String,
}

#[derive(Serialize)]
pub struct ApiResponse {
    pub status: String,
    pub message: String,
}

#[derive(Serialize)]
pub struct BalanceResponse {
    pub account: String,
    pub balance: u64,
}

#[derive(Serialize)]
pub struct MiningResponse {
    pub status: String,
    pub message: String,
    pub block_index: u64,
    pub transactions_count: usize,
}

#[derive(Serialize)]
pub struct ProposeBlockResponse {
    pub status: String,
    pub message: String,
    pub selected_validator: Option<String>,
    pub block: Option<Block>,
    pub block_hash: Option<String>,
    /// VRF input the selected validator must prove over
    /// (hikma-wallet vrf-prove).
    pub vrf_input: Option<String>,
}

#[derive(Serialize)]
pub struct BlockchainStats {
    pub total_blocks: usize,
    pub pending_transactions: usize,
    pub difficulty: usize,
    pub is_valid: bool,
    pub latest_hash: String,
    pub finalized_height: u64,
    pub finality_depth: u64,
    pub state_root: String,
    pub total_supply: u64,
    pub burned: u64,
}

#[derive(Deserialize, Default)]
pub struct PaginationQuery {
    pub offset: Option<usize>,
    pub limit: Option<usize>,
}

#[derive(Serialize)]
pub struct ExplorerOverview {
    pub total_blocks: usize,
    pub finalized_height: u64,
    pub pending_transactions: usize,
    pub difficulty: usize,
    pub latest_hash: String,
    pub peers: usize,
    pub validators: usize,
    pub chain_valid: bool,
}

#[derive(Clone, Serialize)]
pub struct ExplorerBlockSummary {
    pub index: u64,
    pub hash: String,
    pub previous_hash: String,
    pub timestamp: String,
    pub tx_count: usize,
    pub validator: Option<String>,
    pub difficulty: usize,
    pub nonce: u64,
}

#[derive(Clone, Serialize)]
pub struct ExplorerBlockDetail {
    pub block: Block,
    pub pow_valid: bool,
}

#[derive(Serialize)]
pub struct ExplorerBlockListResponse {
    pub total: usize,
    pub offset: usize,
    pub limit: usize,
    pub blocks: Vec<ExplorerBlockSummary>,
}

#[derive(Serialize)]
pub struct ExplorerSearchResponse {
    pub query: String,
    pub block_by_hash: Option<ExplorerBlockDetail>,
    pub block_by_index: Option<ExplorerBlockDetail>,
    pub pending_matches: Vec<Transaction>,
}

#[derive(Serialize)]
pub struct ValidationResponse {
    pub is_valid: bool,
    pub message: String,
    pub details: Option<String>,
}

#[derive(Serialize)]
pub struct DifficultyResponse {
    pub current_difficulty: usize,
}

#[derive(Serialize)]
pub struct StakeResponse {
    pub status: String,
    pub message: String,
    pub total_stake: u64,
}

#[derive(Serialize)]
pub struct ValidatorInfo {
    pub address: String,
    pub stake: u64,
    pub public_key: Option<String>,
}

#[derive(Serialize)]
pub struct GovernanceResponse {
    pub slash_percent: u64,
    pub finality_depth: u64,
}

#[derive(Serialize)]
pub struct SlashEvidenceResponse {
    pub status: String,
    pub message: String,
    pub slashed_amount: u64,
}

#[derive(Serialize)]
pub struct NonceStateResponse {
    pub account: String,
    pub next_nonce: u64,
}

#[derive(Serialize)]
pub struct StateSummaryResponse {
    pub height: u64,
    pub state_root: String,
    pub total_supply: u64,
    pub burned: u64,
    pub staked: u64,
    pub validators: usize,
    pub accounts: usize,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Metrics {
    pub blocks_mined: u64,
    pub blocks_received: u64,
    pub blocks_rejected: u64,
    pub reorgs: u64,
    pub transactions_received: u64,
    pub peers_registered: u64,
    pub slashes_submitted: u64,
    pub gossip_sent: u64,
    pub gossip_failed: u64,
    pub protocol_messages_received: u64,
    pub protocol_messages_rejected: u64,
}

async fn persist_state(state: &AppState) -> Result<(), String> {
    let chain = state.chain.lock().await;
    let contracts = state.contracts.lock().await;
    let pending = state.pending_transactions.lock().await;
    let peers = state.peers.lock().await;
    let governance = state.governance.lock().await;
    let slash_evidence = state.slash_evidence.lock().await;

    let snapshot = AppSnapshot {
        chain: chain.clone(),
        contracts: contracts.clone(),
        pending_transactions: pending.clone(),
        peers: peers.clone(),
        governance: governance.clone(),
        slash_evidence: slash_evidence.clone(),
    };

    save_state(&snapshot).map_err(|err| format!("Failed to save state: {}", err))
}

async fn gossip_blocks(state: &AppState, blocks: Vec<Block>) -> Result<(), String> {
    let targets = {
        let peers = state.peers.lock().await;
        peers.clone()
    };

    let mut sent = 0u64;
    let mut failed = 0u64;

    for block in blocks {
        let (ok, err) = state
            .p2p_service
            .broadcast_block(targets.clone(), block)
            .await;
        sent += ok;
        failed += err;
    }

    let mut metrics = state.metrics.lock().await;
    metrics.gossip_sent += sent;
    metrics.gossip_failed += failed;

    Ok(())
}

async fn gossip_transaction(state: &AppState, transaction: Transaction) {
    let targets = {
        let peers = state.peers.lock().await;
        peers.clone()
    };
    if targets.is_empty() {
        return;
    }
    let (sent, failed) = state
        .p2p_service
        .broadcast_transaction(targets, transaction)
        .await;
    let mut metrics = state.metrics.lock().await;
    metrics.gossip_sent += sent;
    metrics.gossip_failed += failed;
}

/// Constant-time string comparison (HM-07): token checks must not leak
/// prefix-match timing information.
fn constant_time_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut result = 0u8;
    for (x, y) in a.bytes().zip(b.bytes()) {
        result |= x ^ y;
    }
    result == 0
}

fn authorize_p2p(headers: &HeaderMap, state: &AppState) -> bool {
    if state.p2p_tokens.is_empty() {
        return false;
    }
    let Some(supplied) = headers.get("x-p2p-token").and_then(|v| v.to_str().ok()) else {
        return false;
    };
    state.p2p_tokens.iter().any(|t| constant_time_eq(t, supplied))
}

fn authorize_admin(headers: &HeaderMap, state: &AppState) -> bool {
    if state.admin_tokens.is_empty() {
        return false;
    }
    let Some(supplied) = headers.get("x-admin-token").and_then(|v| v.to_str().ok()) else {
        return false;
    };
    state.admin_tokens.iter().any(|t| constant_time_eq(t, supplied))
}

/// Chain state with all queued (not yet mined) transactions applied — the
/// view against which new submissions are validated so queued nonces and
/// balances chain correctly.
fn project_pending_state(chain_state: &ChainState, pending: &[Transaction]) -> ChainState {
    let mut projected = chain_state.clone();
    for tx in pending {
        let _ = projected.apply_transaction(tx);
    }
    projected
}

/// Validate and queue a transaction into the pending pool, then gossip it.
/// The transaction must be statelessly valid AND apply cleanly on top of the
/// chain state plus everything already queued.
async fn queue_transaction(state: &AppState, tx: Transaction) -> Result<(), String> {
    if tx.transaction_type == TransactionType::Reward {
        return Err("Reward transactions cannot be submitted".to_string());
    }
    tx.verify_for_block("__queue__")
        .map_err(|err| format!("Invalid transaction: {}", err))?;

    {
        let chain = state.chain.lock().await;
        let mut pending = state.pending_transactions.lock().await;

        if pending.iter().any(|queued| queued.id == tx.id) {
            return Ok(()); // duplicate gossip — already queued
        }

        let mut projected = project_pending_state(&chain.state, &pending);
        projected
            .apply_transaction(&tx)
            .map_err(|err| format!("Transaction not applicable: {}", err))?;

        pending.push(tx.clone());
    }

    let _ = persist_state(state).await;

    let state_clone = state.clone();
    tokio::spawn(async move {
        gossip_transaction(&state_clone, tx).await;
    });
    Ok(())
}

pub fn api_routes() -> Router<AppState> {
    Router::new()
        // Certificate routes
        .route("/certificates/issue", post(issue_certificate))
        .route("/certificates/verify", post(verify_certificate))
        .route("/certificates/attest", post(attest_certificate))
        // On-chain verifiable credentials (Proof-of-Credential)
        .route("/credentials/issue", post(issue_credential))
        .route("/credentials/revoke", post(revoke_credential))
        .route("/credentials/{id}", get(get_credential))
        .route("/credentials/{id}/proof", get(get_credential_proof))
        // Token routes
        .route("/tokens/transfer", post(transfer_tokens))
        .route("/tokens/faucet", post(faucet_tokens))
        .route("/tokens/balance/{account}", get(get_token_balance))
        .route("/tokens/nonce/{account}", get(get_account_nonce))
        // Blockchain routes
        .route("/blocks", get(get_blocks))
        .route("/blocks/{index}", get(get_block_by_index))
        .route("/blockchain/stats", get(get_blockchain_stats))
        .route("/blockchain/state", get(get_state_summary))
        // Mining routes
        .route("/mine", post(mine_block))
        .route("/mine/propose", post(propose_block))
        .route("/mine/submit", post(submit_block))
        .route("/mining/difficulty", get(get_mining_difficulty))
        .route("/mining/difficulty", post(set_mining_difficulty))
        // Validation routes
        .route("/blockchain/validate", get(validate_blockchain))
        .route("/blocks/{index}/validate", get(validate_block))
        .route("/validate", get(validate_chain)) // Tutorial compatibility
        // Transaction routes
        .route("/transactions/pending", get(get_pending_transactions))
        // Explorer routes (structured, pagination and search)
        .route("/explorer/overview", get(get_explorer_overview))
        .route("/explorer/blocks", get(get_explorer_blocks))
        .route("/explorer/blocks/index/{index}", get(get_explorer_block_by_index))
        .route("/explorer/blocks/hash/{hash}", get(get_explorer_block_by_hash))
        .route("/explorer/search/{query}", get(search_explorer))
        .route("/explorer/transactions/pending", get(get_pending_transactions_structured))
        // Staking routes
        .route("/staking/deposit", post(stake_tokens))
        .route("/staking/withdraw", post(withdraw_stake))
        .route("/staking/validators", get(list_validators))
        // P2P routes
        .route("/p2p/peers", get(list_peers))
        .route("/p2p/peers/register", post(register_peer))
        .route("/p2p/block", post(receive_block))
        .route("/p2p/blocks", post(receive_blocks))
        .route("/p2p/chain", get(get_p2p_chain))
        .route("/p2p/protocol", post(receive_protocol_message))
        // Governance & slashing routes
        .route("/governance/config", get(get_governance))
        .route("/governance/config", post(update_governance))
        .route("/slashing/evidence", post(submit_slash_evidence))
        .route("/slashing/evidence", get(list_slash_evidence))
        .route("/slashing/equivocation", post(submit_equivocation))
        // Metrics
        .route("/metrics", get(get_metrics))
}

fn sanitize_pagination(input: PaginationQuery) -> (usize, usize) {
    let offset = input.offset.unwrap_or(0);
    let limit = input.limit.unwrap_or(20).clamp(1, 100);
    (offset, limit)
}

fn to_block_summary(block: &Block) -> ExplorerBlockSummary {
    ExplorerBlockSummary {
        index: block.index,
        hash: block.hash.clone(),
        previous_hash: block.previous_hash.clone(),
        timestamp: block.timestamp.to_rfc3339(),
        tx_count: block.transactions.len(),
        validator: block.validator.clone(),
        difficulty: block.difficulty,
        nonce: block.nonce,
    }
}

fn to_block_detail(block: &Block) -> ExplorerBlockDetail {
    ExplorerBlockDetail {
        block: block.clone(),
        pow_valid: block.has_valid_pow(),
    }
}

// ===== CERTIFICATE ENDPOINTS =====

async fn issue_certificate(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<CertificateRequest>,
) -> Json<ApiResponse> {
    if !authorize_admin(&headers, &state) {
        return Json(ApiResponse {
            status: "error".to_string(),
            message: "Unauthorized: certificate issuance requires the admin token".to_string(),
        });
    }

    // Update contract state
    let mut contracts = state.contracts.lock().await;
    contracts.issue_certificate(&payload.id, &payload.issued_to, &payload.description);
    drop(contracts);

    // Anchor the issuance on-chain
    let transaction = Transaction::new(
        None,
        payload.issued_to.clone(),
        0,
        TransactionType::Certificate,
    );

    let mut pending = state.pending_transactions.lock().await;
    pending.push(transaction);
    drop(pending);

    let _ = persist_state(&state).await;

    Json(ApiResponse {
        status: "success".to_string(),
        message: format!(
            "Certificate {} issued to {} and added to pending transactions",
            payload.id, payload.issued_to
        ),
    })
}

/// Read-only certificate lookup.
async fn verify_certificate(
    State(state): State<AppState>,
    Json(payload): Json<VerifyCertificateRequest>,
) -> Json<ApiResponse> {
    let contracts = state.contracts.lock().await;
    match contracts.certificates.get(&payload.id) {
        Some(cert) => Json(ApiResponse {
            status: "success".to_string(),
            message: format!(
                "Certificate {} issued to {} (attested: {})",
                cert.id, cert.issued_to, cert.verified
            ),
        }),
        None => Json(ApiResponse {
            status: "error".to_string(),
            message: format!("Certificate {} not found", payload.id),
        }),
    }
}

/// Admin-gated attestation: marks a certificate as verified.
async fn attest_certificate(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<VerifyCertificateRequest>,
) -> Json<ApiResponse> {
    if !authorize_admin(&headers, &state) {
        return Json(ApiResponse {
            status: "error".to_string(),
            message: "Unauthorized: certificate attestation requires the admin token".to_string(),
        });
    }

    let mut contracts = state.contracts.lock().await;
    let success = contracts.verify_certificate(&payload.id);
    drop(contracts);

    let _ = persist_state(&state).await;

    Json(ApiResponse {
        status: if success { "success" } else { "error" }.to_string(),
        message: if success {
            format!("Certificate {} attested", payload.id)
        } else {
            format!("Certificate {} not found", payload.id)
        },
    })
}

// ===== ON-CHAIN VERIFIABLE CREDENTIALS (Proof-of-Credential) =====

#[derive(Serialize)]
pub struct CredentialResponse {
    pub status: String,
    pub message: String,
    pub credential: Option<crate::blockchain::state::CredentialRecord>,
}

#[derive(Serialize)]
pub struct CredentialProofResponse {
    pub status: String,
    pub credential: Option<crate::blockchain::state::CredentialRecord>,
    /// Chain height at which this proof was produced.
    pub height: u64,
    /// State root committing to the credential registry at `height`.
    pub state_root: String,
    /// Hash of the block that committed `state_root` (PoW + validator
    /// signed) — the anchor a third party checks against the network.
    pub block_hash: String,
}

/// Queue a signed credential action (issue or revoke) as an on-chain
/// transaction. Permissionless: any account can issue credentials it signs;
/// only the on-chain issuer can revoke.
async fn queue_credential_action(
    state: &AppState,
    payload: CredentialWriteRequest,
    revoke: bool,
) -> Json<CredentialResponse> {
    let action = CredentialAction {
        id: payload.id.clone(),
        subject: payload.subject.clone(),
        data_hash: payload.data_hash.clone(),
        revoke,
    };

    let mut tx = Transaction::new(
        Some(payload.issuer.clone()),
        payload.subject.clone(),
        0,
        TransactionType::Certificate,
    );
    tx.nonce = payload.nonce;
    tx.public_key = payload.public_key.clone();
    tx.signature = payload.signature.clone();
    tx.credential = Some(action);

    match queue_transaction(state, tx).await {
        Ok(()) => Json(CredentialResponse {
            status: "success".to_string(),
            message: format!(
                "Credential {} {} queued; it takes effect when mined into a block",
                payload.id,
                if revoke { "revocation" } else { "issuance" }
            ),
            credential: None,
        }),
        Err(message) => Json(CredentialResponse {
            status: "error".to_string(),
            message,
            credential: None,
        }),
    }
}

async fn issue_credential(
    State(state): State<AppState>,
    Json(payload): Json<CredentialWriteRequest>,
) -> Json<CredentialResponse> {
    queue_credential_action(&state, payload, false).await
}

async fn revoke_credential(
    State(state): State<AppState>,
    Json(payload): Json<CredentialWriteRequest>,
) -> Json<CredentialResponse> {
    queue_credential_action(&state, payload, true).await
}

async fn get_credential(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Json<CredentialResponse> {
    let chain = state.chain.lock().await;
    match chain.state.credentials.get(&id) {
        Some(record) => Json(CredentialResponse {
            status: "success".to_string(),
            message: if record.revoked {
                format!("Credential {} is REVOKED", id)
            } else {
                format!("Credential {} is valid", id)
            },
            credential: Some(record.clone()),
        }),
        None => Json(CredentialResponse {
            status: "error".to_string(),
            message: format!("Credential {} not found", id),
            credential: None,
        }),
    }
}

/// Portable credential proof: the record plus the chain commitment
/// (height, state root, committing block hash). A third party verifies the
/// state root against any honest node — or replays the chain — without
/// trusting this node.
async fn get_credential_proof(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Json<CredentialProofResponse> {
    let chain = state.chain.lock().await;
    let credential = chain.state.credentials.get(&id).cloned();
    Json(CredentialProofResponse {
        status: if credential.is_some() {
            "success".to_string()
        } else {
            "error".to_string()
        },
        credential,
        height: chain.blocks.len() as u64 - 1,
        state_root: chain.state.state_root(),
        block_hash: chain.latest_hash(),
    })
}

// ===== TOKEN ENDPOINTS =====

async fn transfer_tokens(
    State(state): State<AppState>,
    Json(payload): Json<TokenTransferRequest>,
) -> Json<ApiResponse> {
    if payload.amount == 0 || payload.to.trim().is_empty() {
        return Json(ApiResponse {
            status: "error".to_string(),
            message: "Transfer requires a recipient and a non-zero amount".to_string(),
        });
    }

    let mut tx = Transaction::new(
        Some(payload.from.clone()),
        payload.to.clone(),
        payload.amount,
        TransactionType::Transfer,
    );
    tx.nonce = payload.nonce;
    tx.public_key = payload.public_key.clone();
    tx.signature = payload.signature.clone();

    match queue_transaction(&state, tx).await {
        Ok(()) => Json(ApiResponse {
            status: "success".to_string(),
            message: format!(
                "Transfer of {} from {} to {} queued; it executes when mined into a block",
                payload.amount, payload.from, payload.to
            ),
        }),
        Err(message) => Json(ApiResponse {
            status: "error".to_string(),
            message,
        }),
    }
}

/// Dev/test faucet: a signed transfer from the treasury account, available
/// only when this node holds the treasury key (admin-gated).
async fn faucet_tokens(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<FaucetRequest>,
) -> Json<ApiResponse> {
    if !authorize_admin(&headers, &state) {
        return Json(ApiResponse {
            status: "error".to_string(),
            message: "Unauthorized: faucet requires the admin token".to_string(),
        });
    }
    if payload.amount == 0 || payload.to.trim().is_empty() {
        return Json(ApiResponse {
            status: "error".to_string(),
            message: "Faucet requires a recipient and a non-zero amount".to_string(),
        });
    }
    let treasury = match &state.treasury_key {
        Some(key) => key.clone(),
        None => {
            return Json(ApiResponse {
                status: "error".to_string(),
                message: "Faucet disabled: this node holds no treasury key \
                          (set TREASURY_PRIVATE_KEY on dev networks)"
                    .to_string(),
            });
        }
    };

    // Next nonce = chain nonce plus queued treasury transactions.
    let nonce = {
        let chain = state.chain.lock().await;
        let pending = state.pending_transactions.lock().await;
        let projected = project_pending_state(&chain.state, &pending);
        projected.nonce_of(&treasury.address) + 1
    };

    let message =
        Transaction::transfer_signing_message(&treasury.address, &payload.to, payload.amount, nonce);
    let signature = match pos::sign_message(&message, &treasury.private_key) {
        Ok(value) => value,
        Err(err) => {
            return Json(ApiResponse {
                status: "error".to_string(),
                message: format!("Failed to sign faucet transfer: {}", err),
            });
        }
    };

    let mut tx = Transaction::new(
        Some(treasury.address.clone()),
        payload.to.clone(),
        payload.amount,
        TransactionType::Transfer,
    );
    tx.nonce = nonce;
    tx.public_key = Some(treasury.public_key.clone());
    tx.signature = Some(signature);

    match queue_transaction(&state, tx).await {
        Ok(()) => Json(ApiResponse {
            status: "success".to_string(),
            message: format!(
                "Faucet transfer of {} to {} queued; it executes when mined",
                payload.amount, payload.to
            ),
        }),
        Err(message) => Json(ApiResponse {
            status: "error".to_string(),
            message,
        }),
    }
}

async fn get_token_balance(
    State(state): State<AppState>,
    Path(account): Path<String>,
) -> Json<BalanceResponse> {
    let chain = state.chain.lock().await;
    let balance = chain.state.balance_of(&account);
    Json(BalanceResponse { account, balance })
}

async fn get_account_nonce(
    State(state): State<AppState>,
    Path(account): Path<String>,
) -> Json<NonceStateResponse> {
    let chain = state.chain.lock().await;
    let pending = state.pending_transactions.lock().await;
    let projected = project_pending_state(&chain.state, &pending);
    Json(NonceStateResponse {
        next_nonce: projected.nonce_of(&account) + 1,
        account,
    })
}

// ===== BLOCKCHAIN ENDPOINTS =====

async fn get_blocks(State(state): State<AppState>) -> Json<Vec<String>> {
    let chain = state.chain.lock().await;
    let block_data: Vec<String> = chain.blocks.iter().map(|b| format!("{:?}", b)).collect();
    Json(block_data)
}

async fn get_block_by_index(
    State(state): State<AppState>,
    Path(index): Path<usize>,
) -> Json<Option<String>> {
    let chain = state.chain.lock().await;

    if index < chain.blocks.len() {
        Json(Some(format!("{:?}", chain.blocks[index])))
    } else {
        Json(None)
    }
}

async fn get_blockchain_stats(State(state): State<AppState>) -> Json<BlockchainStats> {
    let chain = state.chain.lock().await;
    let pending = state.pending_transactions.lock().await;
    let governance = state.governance.lock().await;

    Json(BlockchainStats {
        total_blocks: chain.blocks.len(),
        pending_transactions: pending.len(),
        difficulty: chain.difficulty,
        is_valid: chain.quick_integrity(),
        latest_hash: chain.latest_hash(),
        finalized_height: chain.finalized_height,
        finality_depth: governance.finality_depth,
        state_root: chain.state.state_root(),
        total_supply: chain.state.total_supply,
        burned: chain.state.burned,
    })
}

async fn get_state_summary(State(state): State<AppState>) -> Json<StateSummaryResponse> {
    let chain = state.chain.lock().await;
    let staked = chain
        .state
        .balance_of(crate::blockchain::state::STAKING_POOL_ACCOUNT);
    Json(StateSummaryResponse {
        height: chain.blocks.len() as u64 - 1,
        state_root: chain.state.state_root(),
        total_supply: chain.state.total_supply,
        burned: chain.state.burned,
        staked,
        validators: chain.state.validator_set().len(),
        accounts: chain.state.balances.len(),
    })
}

async fn get_explorer_overview(State(state): State<AppState>) -> Json<ExplorerOverview> {
    let chain = state.chain.lock().await;
    let pending = state.pending_transactions.lock().await;
    let peers = state.peers.lock().await;

    Json(ExplorerOverview {
        total_blocks: chain.blocks.len(),
        finalized_height: chain.finalized_height,
        pending_transactions: pending.len(),
        difficulty: chain.difficulty,
        latest_hash: chain.latest_hash(),
        peers: peers.len(),
        validators: chain.state.validator_set().len(),
        chain_valid: chain.quick_integrity(),
    })
}

async fn get_explorer_blocks(
    State(state): State<AppState>,
    Query(pagination): Query<PaginationQuery>,
) -> Json<ExplorerBlockListResponse> {
    let chain = state.chain.lock().await;
    let (offset, limit) = sanitize_pagination(pagination);
    let total = chain.blocks.len();
    let safe_offset = offset.min(total);
    let end = (safe_offset + limit).min(total);

    // Return latest blocks first for explorer UX.
    let mut blocks: Vec<ExplorerBlockSummary> = chain
        .blocks
        .iter()
        .rev()
        .skip(safe_offset)
        .take(end.saturating_sub(safe_offset))
        .map(to_block_summary)
        .collect();

    blocks.sort_by(|a, b| b.index.cmp(&a.index));

    Json(ExplorerBlockListResponse {
        total,
        offset: safe_offset,
        limit,
        blocks,
    })
}

async fn get_explorer_block_by_index(
    State(state): State<AppState>,
    Path(index): Path<usize>,
) -> Json<Option<ExplorerBlockDetail>> {
    let chain = state.chain.lock().await;
    let found = chain.blocks.get(index).map(to_block_detail);
    Json(found)
}

async fn get_explorer_block_by_hash(
    State(state): State<AppState>,
    Path(hash): Path<String>,
) -> Json<Option<ExplorerBlockDetail>> {
    // Security hardening: limit path length for hash lookup.
    if hash.len() > 128 || hash.is_empty() {
        return Json(None);
    }
    let chain = state.chain.lock().await;
    let found = chain.blocks.iter().find(|block| block.hash == hash).map(to_block_detail);
    Json(found)
}

async fn search_explorer(
    State(state): State<AppState>,
    Path(query): Path<String>,
) -> Json<ExplorerSearchResponse> {
    if query.len() > 128 {
        return Json(ExplorerSearchResponse {
            query,
            block_by_hash: None,
            block_by_index: None,
            pending_matches: Vec::new(),
        });
    }

    let chain = state.chain.lock().await;
    let pending = state.pending_transactions.lock().await;

    let block_by_hash = chain
        .blocks
        .iter()
        .find(|block| block.hash == query || block.hash.starts_with(&query))
        .map(to_block_detail);

    let block_by_index = query
        .parse::<usize>()
        .ok()
        .and_then(|index| chain.blocks.get(index))
        .map(to_block_detail);

    let pending_matches: Vec<Transaction> = pending
        .iter()
        .filter(|tx| {
            tx.id == query
                || tx.id.starts_with(&query)
                || tx.to.contains(&query)
                || tx
                    .from
                    .as_ref()
                    .is_some_and(|value| value.contains(&query))
        })
        .cloned()
        .collect();

    Json(ExplorerSearchResponse {
        query,
        block_by_hash,
        block_by_index,
        pending_matches,
    })
}

// ===== MINING ENDPOINTS =====

/// Everything needed to build the next block for the PoS-selected validator.
struct BlockPlan {
    validator: String,
    public_key: String,
    slot_input: String,
    transactions: Vec<String>,
    state_root: String,
    included_ids: Vec<String>,
}

/// Select the validator for the next slot, execute the pending transactions
/// against the current chain state, and commit to the resulting state root.
fn plan_block(chain: &Blockchain, pending: &[Transaction]) -> Result<BlockPlan, String> {
    let has_only_genesis = chain.blocks.len() == 1;
    if pending.is_empty() && !has_only_genesis {
        return Err("No pending transactions to mine".to_string());
    }

    let validator_set = chain.state.validator_set();
    // Slot seed from the VRF randomness beacon: unbiasable by grinding.
    let slot_input = chain.next_slot_input();
    let validator = pos::select_staker_with_seed(&slot_input, &validator_set)
        .ok_or_else(|| "No validators available. Stake tokens to become a validator.".to_string())?;
    let public_key = chain
        .state
        .stakers
        .get(&validator)
        .map(|info| info.public_key.clone())
        .ok_or_else(|| "Selected validator not registered".to_string())?;

    // Execute pending transactions in order; skip any that no longer apply.
    let mut post_state = chain.state.clone();
    let mut transactions = Vec::with_capacity(pending.len() + 1);
    let mut included_ids = Vec::with_capacity(pending.len());
    for tx in pending {
        if tx.verify_for_block(&validator).is_err() {
            continue;
        }
        if post_state.apply_transaction(tx).is_err() {
            continue;
        }
        transactions.push(
            serde_json::to_string(tx)
                .map_err(|err| format!("Failed to serialize transaction: {}", err))?,
        );
        included_ids.push(tx.id.clone());
    }

    let reward = Transaction::new_reward(&validator);
    post_state
        .apply_transaction(&reward)
        .map_err(|err| format!("Failed to apply reward: {}", err))?;
    transactions.push(
        serde_json::to_string(&reward)
            .map_err(|err| format!("Failed to serialize reward transaction: {}", err))?,
    );

    Ok(BlockPlan {
        validator,
        public_key,
        slot_input,
        transactions,
        state_root: post_state.state_root(),
        included_ids,
    })
}

/// Remove pending transactions that were included in accepted blocks or can
/// no longer apply (their nonce was consumed on-chain).
async fn prune_pending(state: &AppState, accepted: &[Block]) {
    let included_ids: HashSet<String> = accepted
        .iter()
        .flat_map(|block| block.transactions.iter())
        .filter_map(|tx_str| serde_json::from_str::<Transaction>(tx_str).ok())
        .map(|tx| tx.id)
        .collect();

    let chain = state.chain.lock().await;
    let mut pending = state.pending_transactions.lock().await;
    pending.retain(|tx| {
        if included_ids.contains(&tx.id) {
            return false;
        }
        match (&tx.from, tx.nonce) {
            (Some(from), nonce) if nonce > 0 => nonce > chain.state.nonce_of(from),
            _ => true,
        }
    });
}

/// Sync with peers after observing a block that does not extend our tip:
/// fetch their chains and adopt the heaviest valid one (fork choice).
async fn sync_with_peers(state: AppState) {
    let peers = {
        let peers = state.peers.lock().await;
        peers.clone()
    };
    if peers.is_empty() {
        return;
    }

    let finality_depth = {
        let governance = state.governance.lock().await;
        governance.finality_depth
    };

    for peer in peers {
        let Some(remote_chain) = state.p2p_service.fetch_chain(&peer).await else {
            continue;
        };

        let adopted = {
            let mut chain = state.chain.lock().await;
            match chain.try_adopt_chain(&remote_chain) {
                Ok(true) => {
                    chain.apply_finality(finality_depth);
                    true
                }
                _ => false,
            }
        };

        if adopted {
            let mut metrics = state.metrics.lock().await;
            metrics.reorgs += 1;
            drop(metrics);
            prune_pending(&state, &[]).await;
            let _ = persist_state(&state).await;
        }
    }
}

/// Mine and sign a block with this node's own validator key. Only succeeds
/// when PoS selected this node's validator for the next slot.
async fn mine_block(State(state): State<AppState>) -> Json<MiningResponse> {
    let finality_depth = {
        let governance = state.governance.lock().await;
        governance.finality_depth
    };

    let validator_key = match &state.validator_key {
        Some(key) => key.clone(),
        None => {
            return Json(MiningResponse {
                status: "error".to_string(),
                message: "This node has no validator key (set VALIDATOR_PRIVATE_KEY). \
                          External validators can use POST /mine/propose and /mine/submit."
                    .to_string(),
                block_index: 0,
                transactions_count: 0,
            });
        }
    };

    let mut pending = state.pending_transactions.lock().await;
    let mut chain = state.chain.lock().await;

    let plan = match plan_block(&chain, &pending) {
        Ok(plan) => plan,
        Err(message) => {
            drop(chain);
            drop(pending);
            let status = if message.contains("No pending") {
                "info"
            } else {
                "error"
            };
            return Json(MiningResponse {
                status: status.to_string(),
                message,
                block_index: 0,
                transactions_count: 0,
            });
        }
    };

    if plan.validator != validator_key.address {
        let message = format!(
            "PoS selected validator {} for this slot; this node's validator is {}. \
             The selected validator must produce the block.",
            plan.validator, validator_key.address
        );
        drop(chain);
        drop(pending);
        return Json(MiningResponse {
            status: "info".to_string(),
            message,
            block_index: 0,
            transactions_count: 0,
        });
    }

    if plan.public_key != validator_key.public_key {
        drop(chain);
        drop(pending);
        return Json(MiningResponse {
            status: "error".to_string(),
            message: "Local validator key does not match this validator's registered public key"
                .to_string(),
            block_index: 0,
            transactions_count: 0,
        });
    }

    let transactions_count = plan.transactions.len();
    let mut block = chain.create_block(
        plan.transactions,
        Some(plan.validator.clone()),
        Some(plan.public_key),
        plan.state_root,
    );

    // VRF contribution for this slot (unique — nothing to grind).
    match vrf::prove(&plan.slot_input, &validator_key.private_key) {
        Ok((output, proof)) => {
            block.vrf_output = Some(output);
            block.vrf_proof = Some(proof);
        }
        Err(message) => {
            drop(chain);
            drop(pending);
            return Json(MiningResponse {
                status: "error".to_string(),
                message: format!("Failed to compute VRF proof: {}", message),
                block_index: 0,
                transactions_count: 0,
            });
        }
    }

    let signature = match pos::sign_block_hash(&block.hash, &validator_key.private_key) {
        Ok(value) => value,
        Err(message) => {
            drop(chain);
            drop(pending);
            return Json(MiningResponse {
                status: "error".to_string(),
                message: format!("Failed to sign block: {}", message),
                block_index: 0,
                transactions_count: 0,
            });
        }
    };
    block.validator_signature = Some(signature);

    let post_state = match chain.validate_block_candidate(&block) {
        Ok(post_state) => post_state,
        Err(message) => {
            drop(chain);
            drop(pending);
            return Json(MiningResponse {
                status: "error".to_string(),
                message: format!("Mined block failed validation: {}", message),
                block_index: 0,
                transactions_count: 0,
            });
        }
    };

    let accepted_block = block.clone();
    chain.commit_block(block, post_state);
    chain.apply_finality(finality_depth);
    let block_index = chain.blocks.len() as u64 - 1;

    // Included transactions leave the pending pool.
    let included: HashSet<String> = plan.included_ids.into_iter().collect();
    pending.retain(|tx| !included.contains(&tx.id));

    drop(chain);
    drop(pending);

    let mut metrics = state.metrics.lock().await;
    metrics.blocks_mined += 1;
    drop(metrics);

    let _ = persist_state(&state).await;

    let state_clone = state.clone();
    tokio::spawn(async move {
        let _ = gossip_blocks(&state_clone, vec![accepted_block]).await;
    });

    Json(MiningResponse {
        status: "success".to_string(),
        message: format!(
            "Successfully mined block {} with {} transactions. Validator: {}",
            block_index, transactions_count, validator_key.address
        ),
        block_index,
        transactions_count,
    })
}

/// Build (and PoW-mine) an unsigned block candidate for the PoS-selected
/// validator. The validator signs `block_hash` offline (hikma-wallet
/// sign-block) and submits the signed block to /mine/submit. Chain state is
/// not modified.
async fn propose_block(State(state): State<AppState>) -> Json<ProposeBlockResponse> {
    let pending = state.pending_transactions.lock().await;
    let chain = state.chain.lock().await;

    let plan = match plan_block(&chain, &pending) {
        Ok(plan) => plan,
        Err(message) => {
            return Json(ProposeBlockResponse {
                status: "error".to_string(),
                message,
                selected_validator: None,
                block: None,
                block_hash: None,
                vrf_input: None,
            });
        }
    };

    let block = chain.create_block(
        plan.transactions,
        Some(plan.validator.clone()),
        Some(plan.public_key),
        plan.state_root,
    );

    let block_hash = block.hash.clone();
    Json(ProposeBlockResponse {
        status: "success".to_string(),
        message: format!(
            "Block candidate created for validator {}. Sign block_hash with the validator's \
             private key (hikma-wallet sign-block), compute the VRF proof over vrf_input \
             (hikma-wallet vrf-prove), set validator_signature, vrf_output, and vrf_proof \
             on the block, and POST it to /mine/submit.",
            plan.validator
        ),
        selected_validator: Some(plan.validator),
        block: Some(block),
        block_hash: Some(block_hash),
        vrf_input: Some(plan.slot_input),
    })
}

/// Accept a fully signed block produced via /mine/propose (or by any
/// validator client). The block passes full consensus validation including
/// state-root verification.
async fn submit_block(
    State(state): State<AppState>,
    Json(block): Json<Block>,
) -> Json<MiningResponse> {
    let finality_depth = {
        let governance = state.governance.lock().await;
        governance.finality_depth
    };

    let mut chain = state.chain.lock().await;
    let post_state = match chain.validate_block_candidate(&block) {
        Ok(post_state) => post_state,
        Err(message) => {
            drop(chain);
            let mut metrics = state.metrics.lock().await;
            metrics.blocks_rejected += 1;
            drop(metrics);
            return Json(MiningResponse {
                status: "error".to_string(),
                message,
                block_index: 0,
                transactions_count: 0,
            });
        }
    };

    let accepted_block = block.clone();
    let transactions_count = block.transactions.len();
    chain.commit_block(block, post_state);
    chain.apply_finality(finality_depth);
    let block_index = chain.blocks.len() as u64 - 1;
    drop(chain);

    prune_pending(&state, std::slice::from_ref(&accepted_block)).await;

    let mut metrics = state.metrics.lock().await;
    metrics.blocks_mined += 1;
    drop(metrics);

    let _ = persist_state(&state).await;

    let state_clone = state.clone();
    let gossip_block = accepted_block.clone();
    tokio::spawn(async move {
        let _ = gossip_blocks(&state_clone, vec![gossip_block]).await;
    });

    Json(MiningResponse {
        status: "success".to_string(),
        message: format!(
            "Signed block accepted at height {}. Validator: {}",
            block_index,
            accepted_block
                .validator
                .as_deref()
                .unwrap_or("unknown")
        ),
        block_index,
        transactions_count,
    })
}

async fn get_mining_difficulty(State(state): State<AppState>) -> Json<DifficultyResponse> {
    let chain = state.chain.lock().await;
    Json(DifficultyResponse {
        current_difficulty: chain.difficulty,
    })
}

async fn set_mining_difficulty(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<DifficultyRequest>,
) -> Json<ApiResponse> {
    if !authorize_admin(&headers, &state) {
        return Json(ApiResponse {
            status: "error".to_string(),
            message: "Unauthorized: changing difficulty requires the admin token".to_string(),
        });
    }
    if !pow::is_difficulty_in_bounds(payload.difficulty) {
        return Json(ApiResponse {
            status: "error".to_string(),
            message: format!(
                "Difficulty must be between {} and {}",
                pow::MIN_DIFFICULTY,
                pow::MAX_DIFFICULTY
            ),
        });
    }

    let mut chain = state.chain.lock().await;
    let old_difficulty = chain.difficulty;
    chain.difficulty = payload.difficulty;
    drop(chain);

    let _ = persist_state(&state).await;

    Json(ApiResponse {
        status: "success".to_string(),
        message: format!(
            "Mining difficulty changed from {} to {}",
            old_difficulty, payload.difficulty
        ),
    })
}

// ===== STAKING ENDPOINTS =====

async fn stake_tokens(
    State(state): State<AppState>,
    Json(payload): Json<StakeRequest>,
) -> Json<StakeResponse> {
    if payload.amount == 0 {
        return Json(StakeResponse {
            status: "error".to_string(),
            message: "Stake amount must be greater than zero".to_string(),
            total_stake: 0,
        });
    }

    let mut tx = Transaction::new(
        Some(payload.address.clone()),
        crate::blockchain::state::STAKING_POOL_ACCOUNT.to_string(),
        payload.amount,
        TransactionType::Stake,
    );
    tx.nonce = payload.nonce;
    tx.public_key = payload.public_key.clone();
    tx.vrf_public_key = payload.vrf_public_key.clone();
    tx.signature = payload.signature.clone();

    match queue_transaction(&state, tx).await {
        Ok(()) => {
            let chain = state.chain.lock().await;
            let total_stake = chain
                .state
                .balance_of(crate::blockchain::state::STAKING_POOL_ACCOUNT);
            Json(StakeResponse {
                status: "success".to_string(),
                message: format!(
                    "Stake of {} for {} queued; the validator activates when the \
                     transaction is mined into a block",
                    payload.amount, payload.address
                ),
                total_stake,
            })
        }
        Err(message) => Json(StakeResponse {
            status: "error".to_string(),
            message,
            total_stake: 0,
        }),
    }
}

async fn withdraw_stake(
    State(state): State<AppState>,
    Json(payload): Json<StakeRequest>,
) -> Json<StakeResponse> {
    if payload.amount == 0 {
        return Json(StakeResponse {
            status: "error".to_string(),
            message: "Withdraw amount must be greater than zero".to_string(),
            total_stake: 0,
        });
    }

    let mut tx = Transaction::new(
        Some(payload.address.clone()),
        payload.address.clone(),
        payload.amount,
        TransactionType::Withdraw,
    );
    tx.nonce = payload.nonce;
    tx.signature = payload.signature.clone();

    match queue_transaction(&state, tx).await {
        Ok(()) => {
            let chain = state.chain.lock().await;
            let total_stake = chain
                .state
                .balance_of(crate::blockchain::state::STAKING_POOL_ACCOUNT);
            Json(StakeResponse {
                status: "success".to_string(),
                message: format!(
                    "Withdrawal of {} for {} queued; it executes when mined into a block",
                    payload.amount, payload.address
                ),
                total_stake,
            })
        }
        Err(message) => Json(StakeResponse {
            status: "error".to_string(),
            message,
            total_stake: 0,
        }),
    }
}

async fn list_validators(State(state): State<AppState>) -> Json<Vec<ValidatorInfo>> {
    let chain = state.chain.lock().await;
    let validators = chain
        .state
        .validator_set()
        .into_iter()
        .map(|staker| ValidatorInfo {
            address: staker.address,
            stake: staker.stake,
            public_key: staker.public_key,
        })
        .collect();
    Json(validators)
}

// ===== P2P ENDPOINTS =====

async fn list_peers(State(state): State<AppState>, headers: HeaderMap) -> Json<Vec<String>> {
    if !authorize_p2p(&headers, &state) {
        return Json(Vec::new());
    }
    let peers = state.peers.lock().await;
    Json(peers.clone())
}

async fn register_peer(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<PeerRequest>,
) -> Json<ApiResponse> {
    if !authorize_p2p(&headers, &state) {
        return Json(ApiResponse {
            status: "error".to_string(),
            message: "Unauthorized peer request".to_string(),
        });
    }
    if payload.address.trim().is_empty() {
        return Json(ApiResponse {
            status: "error".to_string(),
            message: "Peer address cannot be empty".to_string(),
        });
    }

    let mut peers = state.peers.lock().await;
    if !peers.contains(&payload.address) {
        peers.push(payload.address.clone());
        let mut metrics = state.metrics.lock().await;
        metrics.peers_registered += 1;
    }
    drop(peers);
    let _ = persist_state(&state).await;

    Json(ApiResponse {
        status: "success".to_string(),
        message: format!("Registered peer {}", payload.address),
    })
}

/// Serve the full chain to authenticated peers (used for fork choice).
async fn get_p2p_chain(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Blockchain>, StatusCode> {
    if !authorize_p2p(&headers, &state) {
        return Err(StatusCode::UNAUTHORIZED);
    }
    let chain = state.chain.lock().await;
    Ok(Json(chain.clone()))
}

/// Accept one block from a peer; on a tip mismatch, trigger fork-choice
/// sync in the background.
async fn accept_peer_block(state: &AppState, block: Block, finality_depth: u64) -> Result<u64, String> {
    let mut chain = state.chain.lock().await;

    let post_state = match chain.validate_block_candidate(&block) {
        Ok(post_state) => post_state,
        Err(message) => {
            drop(chain);
            if message.contains("chain tip") {
                let state_clone = state.clone();
                tokio::spawn(async move {
                    sync_with_peers(state_clone).await;
                });
            }
            return Err(message);
        }
    };

    let accepted = block.clone();
    chain.commit_block(block, post_state);
    chain.apply_finality(finality_depth);
    let height = chain.blocks.len() as u64 - 1;
    drop(chain);

    prune_pending(state, std::slice::from_ref(&accepted)).await;
    Ok(height)
}

async fn receive_block(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(block): Json<Block>,
) -> Json<ApiResponse> {
    if !authorize_p2p(&headers, &state) {
        return Json(ApiResponse {
            status: "error".to_string(),
            message: "Unauthorized peer request".to_string(),
        });
    }
    let finality_depth = {
        let governance = state.governance.lock().await;
        governance.finality_depth
    };

    match accept_peer_block(&state, block, finality_depth).await {
        Ok(_) => {
            let mut metrics = state.metrics.lock().await;
            metrics.blocks_received += 1;
            drop(metrics);
            let _ = persist_state(&state).await;
            Json(ApiResponse {
                status: "success".to_string(),
                message: "Block accepted".to_string(),
            })
        }
        Err(message) => {
            let mut metrics = state.metrics.lock().await;
            metrics.blocks_rejected += 1;
            drop(metrics);
            Json(ApiResponse {
                status: "error".to_string(),
                message,
            })
        }
    }
}

async fn receive_blocks(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(blocks): Json<Vec<Block>>,
) -> Json<ApiResponse> {
    if !authorize_p2p(&headers, &state) {
        return Json(ApiResponse {
            status: "error".to_string(),
            message: "Unauthorized peer request".to_string(),
        });
    }
    let finality_depth = {
        let governance = state.governance.lock().await;
        governance.finality_depth
    };

    let mut accepted = 0u64;
    for block in blocks {
        if accept_peer_block(&state, block, finality_depth).await.is_ok() {
            accepted += 1;
        } else {
            break;
        }
    }

    if accepted > 0 {
        let mut metrics = state.metrics.lock().await;
        metrics.blocks_received += accepted;
        drop(metrics);
        let _ = persist_state(&state).await;
    }

    Json(ApiResponse {
        status: "success".to_string(),
        message: format!("Accepted {} blocks", accepted),
    })
}

async fn receive_protocol_message(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(envelope): Json<P2PEnvelope>,
) -> Json<ApiResponse> {
    if !authorize_p2p(&headers, &state) {
        let mut metrics = state.metrics.lock().await;
        metrics.protocol_messages_rejected += 1;
        return Json(ApiResponse {
            status: "error".to_string(),
            message: "Unauthorized peer request".to_string(),
        });
    }

    if let Err(message) = envelope.validate(300) {
        let mut metrics = state.metrics.lock().await;
        metrics.protocol_messages_rejected += 1;
        return Json(ApiResponse {
            status: "error".to_string(),
            message,
        });
    }

    // Replay protection: each envelope may only be processed once.
    {
        let mut seen = state.seen_messages.lock().await;
        if !seen.insert(&envelope.message_id) {
            drop(seen);
            let mut metrics = state.metrics.lock().await;
            metrics.protocol_messages_rejected += 1;
            return Json(ApiResponse {
                status: "error".to_string(),
                message: "Duplicate P2P message rejected".to_string(),
            });
        }
    }

    {
        let mut metrics = state.metrics.lock().await;
        metrics.protocol_messages_received += 1;
    }

    match envelope.payload {
        P2PPayload::Ping => Json(ApiResponse {
            status: "success".to_string(),
            message: "pong".to_string(),
        }),
        P2PPayload::PeerAnnounce { address } => {
            if address.trim().is_empty() {
                let mut metrics = state.metrics.lock().await;
                metrics.protocol_messages_rejected += 1;
                return Json(ApiResponse {
                    status: "error".to_string(),
                    message: "Peer address cannot be empty".to_string(),
                });
            }

            let mut peers = state.peers.lock().await;
            if !peers.contains(&address) {
                peers.push(address.clone());
                let mut metrics = state.metrics.lock().await;
                metrics.peers_registered += 1;
            }
            drop(peers);
            let _ = persist_state(&state).await;

            Json(ApiResponse {
                status: "success".to_string(),
                message: format!("Registered peer {}", address),
            })
        }
        P2PPayload::Transaction(transaction) => {
            // Gossiped transaction: validate and queue (no re-gossip — the
            // envelope cache already stops replay storms).
            let already_queued = {
                let pending = state.pending_transactions.lock().await;
                pending.iter().any(|tx| tx.id == transaction.id)
            };
            if already_queued {
                return Json(ApiResponse {
                    status: "success".to_string(),
                    message: "Transaction already queued".to_string(),
                });
            }

            let valid = transaction.verify_for_block("__queue__").is_ok()
                && transaction.transaction_type != TransactionType::Reward;
            if !valid {
                let mut metrics = state.metrics.lock().await;
                metrics.protocol_messages_rejected += 1;
                return Json(ApiResponse {
                    status: "error".to_string(),
                    message: "Invalid gossiped transaction".to_string(),
                });
            }

            {
                let chain = state.chain.lock().await;
                let mut pending = state.pending_transactions.lock().await;
                let mut projected = project_pending_state(&chain.state, &pending);
                if projected.apply_transaction(&transaction).is_err() {
                    drop(pending);
                    drop(chain);
                    let mut metrics = state.metrics.lock().await;
                    metrics.protocol_messages_rejected += 1;
                    return Json(ApiResponse {
                        status: "error".to_string(),
                        message: "Gossiped transaction not applicable".to_string(),
                    });
                }
                pending.push(transaction);
            }

            let mut metrics = state.metrics.lock().await;
            metrics.transactions_received += 1;
            drop(metrics);
            let _ = persist_state(&state).await;

            Json(ApiResponse {
                status: "success".to_string(),
                message: "Transaction queued".to_string(),
            })
        }
        P2PPayload::Block(block) => {
            let finality_depth = {
                let governance = state.governance.lock().await;
                governance.finality_depth
            };
            match accept_peer_block(&state, block, finality_depth).await {
                Ok(_) => {
                    let mut metrics = state.metrics.lock().await;
                    metrics.blocks_received += 1;
                    drop(metrics);
                    let _ = persist_state(&state).await;
                    Json(ApiResponse {
                        status: "success".to_string(),
                        message: "Block accepted".to_string(),
                    })
                }
                Err(message) => {
                    let mut metrics = state.metrics.lock().await;
                    metrics.blocks_rejected += 1;
                    metrics.protocol_messages_rejected += 1;
                    drop(metrics);
                    Json(ApiResponse {
                        status: "error".to_string(),
                        message,
                    })
                }
            }
        }
        P2PPayload::BlockBatch(blocks) => {
            let finality_depth = {
                let governance = state.governance.lock().await;
                governance.finality_depth
            };
            let mut accepted = 0u64;
            for block in blocks {
                if accept_peer_block(&state, block, finality_depth).await.is_ok() {
                    accepted += 1;
                } else {
                    break;
                }
            }
            if accepted > 0 {
                let mut metrics = state.metrics.lock().await;
                metrics.blocks_received += accepted;
                drop(metrics);
                let _ = persist_state(&state).await;
            }
            Json(ApiResponse {
                status: "success".to_string(),
                message: format!("Accepted {} blocks", accepted),
            })
        }
    }
}

// ===== GOVERNANCE & SLASHING ENDPOINTS =====

async fn get_governance(State(state): State<AppState>) -> Json<GovernanceResponse> {
    let governance = state.governance.lock().await;
    Json(GovernanceResponse {
        slash_percent: governance.slash_percent,
        finality_depth: governance.finality_depth,
    })
}

async fn update_governance(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<GovernanceRequest>,
) -> Json<ApiResponse> {
    if !authorize_admin(&headers, &state) {
        return Json(ApiResponse {
            status: "error".to_string(),
            message: "Unauthorized admin request".to_string(),
        });
    }
    if payload.slash_percent == 0 || payload.slash_percent > 100 {
        return Json(ApiResponse {
            status: "error".to_string(),
            message: "slash_percent must be between 1 and 100".to_string(),
        });
    }
    if payload.finality_depth == 0 || payload.finality_depth > 10_000 {
        return Json(ApiResponse {
            status: "error".to_string(),
            message: "finality_depth must be between 1 and 10,000".to_string(),
        });
    }

    let mut governance = state.governance.lock().await;
    governance.slash_percent = payload.slash_percent;
    governance.finality_depth = payload.finality_depth;
    drop(governance);
    let _ = persist_state(&state).await;

    Json(ApiResponse {
        status: "success".to_string(),
        message: format!(
            "Updated slash_percent to {} and finality_depth to {}",
            payload.slash_percent, payload.finality_depth
        ),
    })
}

/// Permissionless equivocation reporting: anyone holding a valid proof that
/// a validator signed two different blocks at the same height can submit it.
/// The slash executes on-chain when the transaction is mined.
async fn submit_equivocation(
    State(state): State<AppState>,
    Json(proof): Json<SlashProof>,
) -> Json<ApiResponse> {
    let offender = match proof.verify() {
        Ok(offender) => offender,
        Err(message) => {
            return Json(ApiResponse {
                status: "error".to_string(),
                message: format!("Invalid equivocation proof: {}", message),
            });
        }
    };

    let mut tx = Transaction::new(None, offender.clone(), 0, TransactionType::Slash);
    tx.slash_proof = Some(proof);

    match queue_transaction(&state, tx).await {
        Ok(()) => {
            let mut metrics = state.metrics.lock().await;
            metrics.slashes_submitted += 1;
            drop(metrics);
            Json(ApiResponse {
                status: "success".to_string(),
                message: format!(
                    "Equivocation proof against {} queued; the slash executes when mined",
                    offender
                ),
            })
        }
        Err(message) => Json(ApiResponse {
            status: "error".to_string(),
            message,
        }),
    }
}

/// Admin diagnostics: evaluate whether a block in the local chain contains
/// slashable behavior and log the evidence. (Stake changes only ever happen
/// on-chain via Slash transactions.)
async fn submit_slash_evidence(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<SlashEvidenceRequest>,
) -> Json<SlashEvidenceResponse> {
    if !authorize_admin(&headers, &state) {
        return Json(SlashEvidenceResponse {
            status: "error".to_string(),
            message: "Unauthorized admin request".to_string(),
            slashed_amount: 0,
        });
    }
    let chain = state.chain.lock().await;
    let evidence = match chain.evaluate_slash_evidence(payload.block_index) {
        Ok(evidence) => evidence,
        Err(message) => {
            return Json(SlashEvidenceResponse {
                status: "error".to_string(),
                message,
                slashed_amount: 0,
            });
        }
    };
    drop(chain);

    let mut evidence_log = state.slash_evidence.lock().await;
    evidence_log.push(crate::persistence::SlashEvidence {
        block_index: payload.block_index,
        reason: evidence.reason.clone(),
        reporter: payload.reporter,
        timestamp: evidence.timestamp,
        slashed_amount: 0,
    });
    drop(evidence_log);
    let _ = persist_state(&state).await;

    Json(SlashEvidenceResponse {
        status: "success".to_string(),
        message: format!(
            "Recorded evidence against validator {}: {}. Submit an equivocation proof \
             to /slashing/equivocation to slash on-chain.",
            evidence.validator, evidence.reason
        ),
        slashed_amount: 0,
    })
}

async fn list_slash_evidence(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Json<Vec<crate::persistence::SlashEvidence>> {
    if !authorize_admin(&headers, &state) {
        return Json(Vec::new());
    }
    let evidence = state.slash_evidence.lock().await;
    Json(evidence.clone())
}

async fn get_metrics(State(state): State<AppState>) -> Json<Metrics> {
    let metrics = state.metrics.lock().await;
    Json(metrics.clone())
}

// ===== VALIDATION ENDPOINTS =====

async fn validate_blockchain(State(state): State<AppState>) -> Json<ValidationResponse> {
    let chain = state.chain.lock().await;
    let (is_valid, details) = chain.validate_report();

    Json(ValidationResponse {
        is_valid,
        message: if is_valid {
            "Blockchain is valid".to_string()
        } else {
            "Blockchain validation failed".to_string()
        },
        details: if is_valid {
            Some(
                "All blocks properly linked with valid PoW, PoS, and state execution".to_string(),
            )
        } else {
            details
        },
    })
}

async fn validate_block(
    State(state): State<AppState>,
    Path(index): Path<usize>,
) -> Json<ValidationResponse> {
    let chain = state.chain.lock().await;

    if index >= chain.blocks.len() {
        return Json(ValidationResponse {
            is_valid: false,
            message: "Block not found".to_string(),
            details: Some(format!("Block index {} does not exist", index)),
        });
    }

    if index == 0 {
        let genesis_valid = chain.blocks[0].has_valid_pow();
        return Json(ValidationResponse {
            is_valid: genesis_valid,
            message: if genesis_valid {
                "Genesis block is valid".to_string()
            } else {
                "Genesis block is invalid".to_string()
            },
            details: Some("Genesis block PoW validation".to_string()),
        });
    }

    let (is_valid, error) = match chain.evaluate_slash_evidence(index as u64) {
        Ok(evidence) => (false, Some(evidence.reason)),
        Err(message) if message.contains("does not contain slashable") => (true, None),
        Err(message) => (false, Some(message)),
    };

    Json(ValidationResponse {
        is_valid,
        message: if is_valid {
            format!("Block {} is valid", index)
        } else {
            format!("Block {} validation failed", index)
        },
        details: if is_valid {
            Some("Block is valid".to_string())
        } else {
            error
        },
    })
}

// Tutorial compatibility endpoint
async fn validate_chain(State(state): State<AppState>) -> Json<ApiResponse> {
    let chain = state.chain.lock().await;
    let is_valid = chain.is_valid();

    Json(ApiResponse {
        status: if is_valid { "success" } else { "error" }.to_string(),
        message: if is_valid {
            "Blockchain is valid.".to_string()
        } else {
            "Blockchain is invalid!".to_string()
        },
    })
}

// ===== TRANSACTION ENDPOINTS =====

async fn get_pending_transactions(State(state): State<AppState>) -> Json<Vec<String>> {
    let pending = state.pending_transactions.lock().await;
    let transaction_strings: Vec<String> = pending.iter().map(|tx| format!("{:?}", tx)).collect();
    Json(transaction_strings)
}

async fn get_pending_transactions_structured(State(state): State<AppState>) -> Json<Vec<Transaction>> {
    let pending = state.pending_transactions.lock().await;
    Json(pending.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blockchain::chain::dev_genesis_private_key;
    use crate::blockchain::transaction::BLOCK_REWARD;
    use axum::http::HeaderValue;

    const ADMIN_TOKEN: &str = "test-admin-token";
    const P2P_TOKEN: &str = "test-p2p-token";

    fn treasury_key() -> LocalValidatorKey {
        LocalValidatorKey::from_private_key(&dev_genesis_private_key()).unwrap()
    }

    fn test_state(with_local_validator: bool) -> AppState {
        std::env::set_var(
            "HIKMALAYER_STATE_PATH",
            format!(
                "{}/hikmalayer-test-state-{}.json",
                std::env::temp_dir().display(),
                uuid::Uuid::new_v4()
            ),
        );
        let treasury = treasury_key();
        AppState {
            chain: Arc::new(Mutex::new(Blockchain::new(2))),
            contracts: Arc::new(Mutex::new(ContractExecutor::new())),
            pending_transactions: Arc::new(Mutex::new(Vec::new())),
            auth_manager: Arc::new(Mutex::new(AuthManager::new())),
            peers: Arc::new(Mutex::new(Vec::new())),
            governance: Arc::new(Mutex::new(GovernanceConfig::default())),
            slash_evidence: Arc::new(Mutex::new(Vec::new())),
            metrics: Arc::new(Mutex::new(Metrics::default())),
            seen_messages: Arc::new(Mutex::new(SeenMessageCache::new(1024))),
            p2p_tokens: vec![P2P_TOKEN.to_string()],
            admin_tokens: vec![ADMIN_TOKEN.to_string()],
            p2p_service: Arc::new(P2PService::new("node-test".to_string(), None).unwrap()),
            validator_key: with_local_validator.then(|| treasury.clone()),
            treasury_key: Some(treasury),
        }
    }

    fn admin_headers() -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert("x-admin-token", HeaderValue::from_static(ADMIN_TOKEN));
        headers
    }

    fn p2p_headers() -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert("x-p2p-token", HeaderValue::from_static(P2P_TOKEN));
        headers
    }

    fn test_wallet(seed: u8) -> (String, String, String) {
        let private_key = hex::encode([seed; 32]);
        let public_key = pos::derive_public_key(&private_key).unwrap();
        let address = pos::derive_address(&public_key).unwrap();
        (address, public_key, private_key)
    }

    /// Mine the next block regardless of which validator is selected, using
    /// the provided keyring.
    async fn mine_next(state: &AppState, keyring: &[(String, String)]) {
        let proposal = propose_block(State(state.clone())).await;
        assert_eq!(proposal.0.status, "success", "{}", proposal.0.message);
        let mut block = proposal.0.block.unwrap();
        let selected = proposal.0.selected_validator.unwrap();
        let vrf_input = proposal.0.vrf_input.unwrap();
        let key = keyring
            .iter()
            .find(|(addr, _)| *addr == selected)
            .map(|(_, key)| key.clone())
            .expect("no key for selected validator");
        // The validator's offline steps: VRF proof + block signature.
        let (vrf_output, vrf_proof) = vrf::prove(&vrf_input, &key).unwrap();
        block.vrf_output = Some(vrf_output);
        block.vrf_proof = Some(vrf_proof);
        block.validator_signature =
            Some(pos::sign_block_hash(&block.hash, &key).unwrap());
        let response = submit_block(State(state.clone()), Json(block)).await;
        assert_eq!(response.0.status, "success", "{}", response.0.message);
    }

    fn base_keyring() -> Vec<(String, String)> {
        let treasury = treasury_key();
        vec![(treasury.address, treasury.private_key)]
    }

    async fn signed_transfer_request(
        state: &AppState,
        from: &(String, String, String),
        to: &str,
        amount: u64,
    ) -> Json<ApiResponse> {
        let nonce = get_account_nonce(State(state.clone()), Path(from.0.clone()))
            .await
            .0
            .next_nonce;
        let message = Transaction::transfer_signing_message(&from.0, to, amount, nonce);
        let signature = pos::sign_message(&message, &from.2).unwrap();
        transfer_tokens(
            State(state.clone()),
            Json(TokenTransferRequest {
                from: from.0.clone(),
                to: to.to_string(),
                amount,
                nonce,
                public_key: Some(from.1.clone()),
                signature: Some(signature),
            }),
        )
        .await
    }

    /// Fund an account from treasury and mine the transfer on-chain.
    async fn fund(state: &AppState, to: &str, amount: u64) {
        let t = treasury_key();
        let from = (t.address.clone(), t.public_key.clone(), t.private_key.clone());
        let response = signed_transfer_request(state, &from, to, amount).await;
        assert_eq!(response.0.status, "success", "{}", response.0.message);
        mine_next(state, &base_keyring()).await;
    }

    /// Fund and stake a new validator on-chain. Returns its wallet.
    async fn register_staker(
        state: &AppState,
        seed: u8,
        stake: u64,
    ) -> (String, String, String) {
        let (address, public_key, private_key) = test_wallet(seed);
        fund(state, &address, stake * 2).await;

        let nonce = get_account_nonce(State(state.clone()), Path(address.clone()))
            .await
            .0
            .next_nonce;
        let vrf_public_key = vrf::derive_vrf_public_key(&private_key).unwrap();
        let message =
            Transaction::stake_signing_message(&address, stake, nonce, &vrf_public_key);
        let signature = pos::sign_message(&message, &private_key).unwrap();
        let response = stake_tokens(
            State(state.clone()),
            Json(StakeRequest {
                address: address.clone(),
                amount: stake,
                public_key: Some(public_key.clone()),
                vrf_public_key: Some(vrf_public_key),
                nonce,
                signature: Some(signature),
            }),
        )
        .await;
        assert_eq!(response.0.status, "success", "{}", response.0.message);

        let mut keyring = base_keyring();
        keyring.push((address.clone(), private_key.clone()));
        mine_next(state, &keyring).await;
        (address, public_key, private_key)
    }

    #[tokio::test]
    async fn transfer_without_signature_is_rejected() {
        let state = test_state(false);
        let response = transfer_tokens(
            State(state),
            Json(TokenTransferRequest {
                from: "hkmnobody".to_string(),
                to: "hkmsomeone".to_string(),
                amount: 10,
                nonce: 1,
                public_key: None,
                signature: None,
            }),
        )
        .await;
        assert_eq!(response.0.status, "error");
        assert!(
            response.0.message.contains("public key") || response.0.message.contains("signature"),
            "{}",
            response.0.message
        );
    }

    #[tokio::test]
    async fn signed_transfer_executes_on_chain_and_replay_fails() {
        let state = test_state(true);
        let t = treasury_key();
        let from = (t.address.clone(), t.public_key.clone(), t.private_key.clone());

        let response = signed_transfer_request(&state, &from, "hkmrecipient", 40).await;
        assert_eq!(response.0.status, "success", "{}", response.0.message);

        // Balance is unchanged until the transfer is mined.
        let balance = get_token_balance(State(state.clone()), Path("hkmrecipient".to_string()))
            .await
            .0
            .balance;
        assert_eq!(balance, 0);

        mine_next(&state, &base_keyring()).await;
        let balance = get_token_balance(State(state.clone()), Path("hkmrecipient".to_string()))
            .await
            .0
            .balance;
        assert_eq!(balance, 40);

        // Replaying the identical signed payload (same nonce) is rejected.
        let message = Transaction::transfer_signing_message(&from.0, "hkmrecipient", 40, 1);
        let signature = pos::sign_message(&message, &from.2).unwrap();
        let replay = transfer_tokens(
            State(state.clone()),
            Json(TokenTransferRequest {
                from: from.0.clone(),
                to: "hkmrecipient".to_string(),
                amount: 40,
                nonce: 1,
                public_key: Some(from.1.clone()),
                signature: Some(signature),
            }),
        )
        .await;
        assert_eq!(replay.0.status, "error");
        assert!(replay.0.message.contains("nonce"), "{}", replay.0.message);
    }

    #[tokio::test]
    async fn transfer_with_wrong_sender_binding_is_rejected() {
        let state = test_state(false);
        let attacker = test_wallet(12);
        let (victim, ..) = test_wallet(13);

        let message = Transaction::transfer_signing_message(&victim, "hkmattacker", 40, 1);
        let signature = pos::sign_message(&message, &attacker.2).unwrap();
        let response = transfer_tokens(
            State(state),
            Json(TokenTransferRequest {
                from: victim,
                to: "hkmattacker".to_string(),
                amount: 40,
                nonce: 1,
                public_key: Some(attacker.1),
                signature: Some(signature),
            }),
        )
        .await;
        assert_eq!(response.0.status, "error");
    }

    #[tokio::test]
    async fn on_chain_staking_and_withdrawal_flow() {
        let state = test_state(true);
        let (address, _, private_key) = register_staker(&state, 15, 50).await;

        let validators = list_validators(State(state.clone())).await;
        assert_eq!(validators.0.len(), 2);

        // Unsigned withdrawal never enters the pool.
        let response = withdraw_stake(
            State(state.clone()),
            Json(StakeRequest {
                address: address.clone(),
                amount: 50,
                public_key: None,
                vrf_public_key: None,
                nonce: 99,
                signature: None,
            }),
        )
        .await;
        assert_eq!(response.0.status, "error");

        // Signed withdrawal queues and executes when mined.
        let nonce = get_account_nonce(State(state.clone()), Path(address.clone()))
            .await
            .0
            .next_nonce;
        let message = Transaction::withdraw_signing_message(&address, 50, nonce);
        let signature = pos::sign_message(&message, &private_key).unwrap();
        let response = withdraw_stake(
            State(state.clone()),
            Json(StakeRequest {
                address: address.clone(),
                amount: 50,
                public_key: None,
                vrf_public_key: None,
                nonce,
                signature: Some(signature),
            }),
        )
        .await;
        assert_eq!(response.0.status, "success", "{}", response.0.message);

        let mut keyring = base_keyring();
        keyring.push((address.clone(), private_key));
        mine_next(&state, &keyring).await;

        let validators = list_validators(State(state.clone())).await;
        assert_eq!(validators.0.len(), 1);
    }

    #[tokio::test]
    async fn mine_with_local_validator_key_end_to_end() {
        let state = test_state(true);
        let treasury = treasury_key();

        let balance_before = get_token_balance(
            State(state.clone()),
            Path(treasury.address.clone()),
        )
        .await
        .0
        .balance;

        let response = mine_block(State(state.clone())).await;
        assert_eq!(response.0.status, "success", "{}", response.0.message);
        assert_eq!(response.0.block_index, 1);

        let chain = state.chain.lock().await;
        assert_eq!(chain.blocks.len(), 2);
        assert!(chain.is_valid());
        drop(chain);

        let balance_after = get_token_balance(
            State(state.clone()),
            Path(treasury.address.clone()),
        )
        .await
        .0
        .balance;
        assert_eq!(balance_after, balance_before + BLOCK_REWARD);
    }

    #[tokio::test]
    async fn propose_and_submit_flow_produces_valid_block() {
        let state = test_state(false);
        mine_next(&state, &base_keyring()).await;

        let chain = state.chain.lock().await;
        assert_eq!(chain.blocks.len(), 2);
        assert!(chain.is_valid());
    }

    #[tokio::test]
    async fn submit_rejects_unsigned_or_forged_blocks() {
        let state = test_state(false);

        let proposal = propose_block(State(state.clone())).await;
        let mut block = proposal.0.block.unwrap();

        // Unsigned submission fails.
        let response = submit_block(State(state.clone()), Json(block.clone())).await;
        assert_eq!(response.0.status, "error");

        // Signature from a non-registered key fails.
        let intruder_key = hex::encode([99u8; 32]);
        block.validator_signature =
            Some(pos::sign_block_hash(&block.hash, &intruder_key).unwrap());
        let response = submit_block(State(state.clone()), Json(block)).await;
        assert_eq!(response.0.status, "error");

        assert_eq!(state.chain.lock().await.blocks.len(), 1);
    }

    #[tokio::test]
    async fn submit_rejects_forged_state_root() {
        let state = test_state(false);
        let treasury = treasury_key();

        let proposal = propose_block(State(state.clone())).await;
        let mut block = proposal.0.block.unwrap();

        // Forge the state root, re-sign honestly — consensus must catch it.
        block.state_root = "forged".to_string();
        // Recompute the PoW so only the state execution check can fail.
        let rebuilt = Block::new(
            block.index,
            block.transactions.clone(),
            block.previous_hash.clone(),
            block.difficulty,
            block.validator.clone(),
            block.validator_public_key.clone(),
            None,
            block.state_root.clone(),
        );
        let mut forged = rebuilt;
        let vrf_input = state.chain.lock().await.next_slot_input();
        let (vrf_output, vrf_proof) =
            vrf::prove(&vrf_input, &treasury.private_key).unwrap();
        forged.vrf_output = Some(vrf_output);
        forged.vrf_proof = Some(vrf_proof);
        forged.validator_signature =
            Some(pos::sign_block_hash(&forged.hash, &treasury.private_key).unwrap());

        let response = submit_block(State(state.clone()), Json(forged)).await;
        assert_eq!(response.0.status, "error");
        assert!(
            response.0.message.contains("state root"),
            "{}",
            response.0.message
        );
    }

    #[tokio::test]
    async fn faucet_requires_admin_and_treasury_key() {
        let state = test_state(true);

        let response = faucet_tokens(
            State(state.clone()),
            HeaderMap::new(),
            Json(FaucetRequest {
                to: "hkmuser".to_string(),
                amount: 10,
            }),
        )
        .await;
        assert_eq!(response.0.status, "error");

        let response = faucet_tokens(
            State(state.clone()),
            admin_headers(),
            Json(FaucetRequest {
                to: "hkmuser".to_string(),
                amount: 10,
            }),
        )
        .await;
        assert_eq!(response.0.status, "success", "{}", response.0.message);

        mine_next(&state, &base_keyring()).await;
        let balance = get_token_balance(State(state.clone()), Path("hkmuser".to_string()))
            .await
            .0
            .balance;
        assert_eq!(balance, 10);

        // Without a treasury key the faucet is disabled.
        let mut no_treasury = test_state(false);
        no_treasury.treasury_key = None;
        let response = faucet_tokens(
            State(no_treasury),
            admin_headers(),
            Json(FaucetRequest {
                to: "hkmuser".to_string(),
                amount: 10,
            }),
        )
        .await;
        assert_eq!(response.0.status, "error");
        assert!(response.0.message.contains("treasury"));
    }

    #[tokio::test]
    async fn difficulty_change_requires_admin_and_bounds() {
        let state = test_state(false);

        let response = set_mining_difficulty(
            State(state.clone()),
            HeaderMap::new(),
            Json(DifficultyRequest { difficulty: 3 }),
        )
        .await;
        assert_eq!(response.0.status, "error");

        let response = set_mining_difficulty(
            State(state.clone()),
            admin_headers(),
            Json(DifficultyRequest { difficulty: 99 }),
        )
        .await;
        assert_eq!(response.0.status, "error");

        let response = set_mining_difficulty(
            State(state.clone()),
            admin_headers(),
            Json(DifficultyRequest { difficulty: 3 }),
        )
        .await;
        assert_eq!(response.0.status, "success");
        assert_eq!(state.chain.lock().await.difficulty, 3);
    }

    #[tokio::test]
    async fn p2p_endpoints_reject_missing_token_and_replays() {
        let state = test_state(false);

        let block = state.chain.lock().await.blocks[0].clone();
        let response = receive_block(State(state.clone()), HeaderMap::new(), Json(block)).await;
        assert_eq!(response.0.status, "error");

        let envelope = P2PEnvelope::new("node-a".to_string(), P2PPayload::Ping);
        let response = receive_protocol_message(
            State(state.clone()),
            p2p_headers(),
            Json(envelope.clone()),
        )
        .await;
        assert_eq!(response.0.status, "success");

        let response =
            receive_protocol_message(State(state.clone()), p2p_headers(), Json(envelope)).await;
        assert_eq!(response.0.status, "error");
        assert!(response.0.message.contains("Duplicate"));
    }

    #[tokio::test]
    async fn gossiped_transactions_are_validated_and_queued() {
        let state = test_state(false);
        let t = treasury_key();

        let nonce = 1;
        let mut tx = Transaction::new(
            Some(t.address.clone()),
            "hkmrecipient".to_string(),
            25,
            TransactionType::Transfer,
        );
        tx.nonce = nonce;
        tx.public_key = Some(t.public_key.clone());
        let message =
            Transaction::transfer_signing_message(&t.address, "hkmrecipient", 25, nonce);
        tx.signature = Some(pos::sign_message(&message, &t.private_key).unwrap());

        let envelope = P2PEnvelope::new(
            "node-b".to_string(),
            P2PPayload::Transaction(tx.clone()),
        );
        let response =
            receive_protocol_message(State(state.clone()), p2p_headers(), Json(envelope)).await;
        assert_eq!(response.0.status, "success", "{}", response.0.message);
        assert_eq!(state.pending_transactions.lock().await.len(), 1);

        // A forged gossiped transaction is rejected.
        let mut forged = tx.clone();
        forged.id = "forged".to_string();
        forged.amount = 9999;
        let envelope =
            P2PEnvelope::new("node-b".to_string(), P2PPayload::Transaction(forged));
        let response =
            receive_protocol_message(State(state.clone()), p2p_headers(), Json(envelope)).await;
        assert_eq!(response.0.status, "error");
    }

    #[tokio::test]
    async fn credential_lifecycle_on_chain() {
        let state = test_state(true);
        let issuer = test_wallet(21);

        // Unsigned issuance never enters the pool.
        let response = issue_credential(
            State(state.clone()),
            Json(CredentialWriteRequest {
                id: "cred-1".to_string(),
                subject: "hkmsubject".to_string(),
                data_hash: "abc123".to_string(),
                revoke: false,
                issuer: issuer.0.clone(),
                nonce: 1,
                public_key: None,
                signature: None,
            }),
        )
        .await;
        assert_eq!(response.0.status, "error");

        // Signed issuance queues and executes on-chain.
        let action = CredentialAction {
            id: "cred-1".to_string(),
            subject: "hkmsubject".to_string(),
            data_hash: "abc123".to_string(),
            revoke: false,
        };
        let message = Transaction::credential_signing_message(&action, 1);
        let signature = pos::sign_message(&message, &issuer.2).unwrap();
        let response = issue_credential(
            State(state.clone()),
            Json(CredentialWriteRequest {
                id: "cred-1".to_string(),
                subject: "hkmsubject".to_string(),
                data_hash: "abc123".to_string(),
                revoke: false,
                issuer: issuer.0.clone(),
                nonce: 1,
                public_key: Some(issuer.1.clone()),
                signature: Some(signature),
            }),
        )
        .await;
        assert_eq!(response.0.status, "success", "{}", response.0.message);
        mine_next(&state, &base_keyring()).await;

        // Readable, with a portable proof bound to the state root.
        let lookup = get_credential(State(state.clone()), Path("cred-1".to_string())).await;
        assert_eq!(lookup.0.status, "success");
        assert!(!lookup.0.credential.as_ref().unwrap().revoked);

        let proof = get_credential_proof(State(state.clone()), Path("cred-1".to_string())).await;
        assert_eq!(proof.0.status, "success");
        let chain = state.chain.lock().await;
        assert_eq!(proof.0.state_root, chain.state.state_root());
        assert_eq!(proof.0.block_hash, chain.latest_hash());
        drop(chain);

        // Only the issuer can revoke: a stranger's signed revoke is queued
        // but rejected by the state machine (never applies).
        let stranger = test_wallet(22);
        let revoke_action = CredentialAction {
            id: "cred-1".to_string(),
            subject: String::new(),
            data_hash: String::new(),
            revoke: true,
        };
        let message = Transaction::credential_signing_message(&revoke_action, 1);
        let signature = pos::sign_message(&message, &stranger.2).unwrap();
        let response = revoke_credential(
            State(state.clone()),
            Json(CredentialWriteRequest {
                id: "cred-1".to_string(),
                subject: String::new(),
                data_hash: String::new(),
                revoke: true,
                issuer: stranger.0.clone(),
                nonce: 1,
                public_key: Some(stranger.1.clone()),
                signature: Some(signature),
            }),
        )
        .await;
        assert_eq!(response.0.status, "error", "{}", response.0.message);

        // The real issuer revokes (nonce 2).
        let message = Transaction::credential_signing_message(&revoke_action, 2);
        let signature = pos::sign_message(&message, &issuer.2).unwrap();
        let response = revoke_credential(
            State(state.clone()),
            Json(CredentialWriteRequest {
                id: "cred-1".to_string(),
                subject: String::new(),
                data_hash: String::new(),
                revoke: true,
                issuer: issuer.0.clone(),
                nonce: 2,
                public_key: Some(issuer.1.clone()),
                signature: Some(signature),
            }),
        )
        .await;
        assert_eq!(response.0.status, "success", "{}", response.0.message);
        mine_next(&state, &base_keyring()).await;

        let lookup = get_credential(State(state.clone()), Path("cred-1".to_string())).await;
        assert!(lookup.0.credential.as_ref().unwrap().revoked);
        assert!(lookup.0.message.contains("REVOKED"));
    }

    #[tokio::test]
    async fn equivocation_endpoint_slashes_on_chain() {
        let state = test_state(true);
        let treasury = treasury_key();

        // Produce two conflicting signed blocks at the next height.
        let (block_a, block_b) = {
            let chain = state.chain.lock().await;
            let reward = Transaction::new_reward(&treasury.address);
            let mut post = chain.state.clone();
            post.apply_transaction(&reward).unwrap();
            let root = post.state_root();
            let make = |memo: &str| {
                let mut block = chain.create_block(
                    vec![serde_json::to_string(&reward).unwrap(), memo.to_string()],
                    Some(treasury.address.clone()),
                    Some(treasury.public_key.clone()),
                    root.clone(),
                );
                block.validator_signature = Some(
                    pos::sign_block_hash(&block.hash, &treasury.private_key).unwrap(),
                );
                block
            };
            (make("fork-a"), make("fork-b"))
        };

        let response = submit_equivocation(
            State(state.clone()),
            Json(SlashProof { block_a, block_b }),
        )
        .await;
        assert_eq!(response.0.status, "success", "{}", response.0.message);

        let stake_before = {
            let chain = state.chain.lock().await;
            chain.state.stakers.get(&treasury.address).unwrap().stake
        };
        mine_next(&state, &base_keyring()).await;
        let chain = state.chain.lock().await;
        let stake_after = chain.state.stakers.get(&treasury.address).unwrap().stake;
        assert_eq!(stake_after, stake_before - stake_before / 10);
        assert!(chain.state.burned > 0);
        assert!(chain.is_valid());
    }
}
