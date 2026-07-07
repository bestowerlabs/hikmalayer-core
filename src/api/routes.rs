use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::Json,
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::{
    auth::AuthManager,
    blockchain::{
        block::Block,
        chain::Blockchain,
        transaction::{Transaction, TransactionType, BLOCK_REWARD},
    },
    consensus::{
        pos::{self, Staker},
        pow,
    },
    contract::contract::ContractExecutor,
    governance::GovernanceConfig,
    p2p::{
        protocol::{P2PEnvelope, P2PPayload, SeenMessageCache},
        service::P2PService,
    },
    persistence::{save_state, AppSnapshot},
    token::fungible::Token,
};

const STAKING_POOL_ACCOUNT: &str = "__staking_pool__";

/// This node's own validator identity, loaded from the local environment.
/// The key never leaves the node and is never accepted over the network.
#[derive(Clone)]
pub struct LocalValidatorKey {
    pub address: String,
    pub public_key: String,
    pub private_key: String,
}

impl LocalValidatorKey {
    pub fn from_private_key(private_key_hex: &str) -> Result<Self, String> {
        let public_key = pos::derive_public_key(private_key_hex)?;
        let address = pos::derive_address(&public_key)?;
        Ok(Self {
            address,
            public_key,
            private_key: private_key_hex.to_string(),
        })
    }
}

#[derive(Clone)]
pub struct AppState {
    pub chain: Arc<Mutex<Blockchain>>,
    pub token: Arc<Mutex<Token>>,
    pub contracts: Arc<Mutex<ContractExecutor>>,
    pub pending_transactions: Arc<Mutex<Vec<Transaction>>>,
    pub auth_manager: Arc<Mutex<AuthManager>>,
    pub stakers: Arc<Mutex<Vec<Staker>>>,
    pub peers: Arc<Mutex<Vec<String>>>,
    pub governance: Arc<Mutex<GovernanceConfig>>,
    pub slash_evidence: Arc<Mutex<Vec<crate::persistence::SlashEvidence>>>,
    pub metrics: Arc<Mutex<Metrics>>,
    pub nonces: Arc<Mutex<HashMap<String, u64>>>,
    pub seen_messages: Arc<Mutex<SeenMessageCache>>,
    pub p2p_token: Option<String>,
    pub admin_token: Option<String>,
    pub p2p_service: Arc<P2PService>,
    pub validator_key: Option<LocalValidatorKey>,
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
pub struct DifficultyRequest {
    pub difficulty: usize,
}

#[derive(Deserialize)]
pub struct StakeRequest {
    pub address: String,
    pub amount: u64,
    pub public_key: Option<String>,
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
    pub slashed: Vec<SlashEvent>,
}

#[derive(Serialize)]
pub struct DifficultyResponse {
    pub current_difficulty: usize,
}

#[derive(Serialize)]
pub struct SlashEvent {
    pub address: String,
    pub amount: u64,
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

#[derive(Debug, Clone, Default, Serialize)]
pub struct Metrics {
    pub blocks_mined: u64,
    pub blocks_received: u64,
    pub blocks_rejected: u64,
    pub reorgs: u64,
    pub peers_registered: u64,
    pub slashes_submitted: u64,
    pub gossip_sent: u64,
    pub gossip_failed: u64,
    pub protocol_messages_received: u64,
    pub protocol_messages_rejected: u64,
}

async fn persist_state(state: &AppState) -> Result<(), String> {
    let chain = state.chain.lock().await;
    let token = state.token.lock().await;
    let contracts = state.contracts.lock().await;
    let pending = state.pending_transactions.lock().await;
    let stakers = state.stakers.lock().await;
    let peers = state.peers.lock().await;
    let governance = state.governance.lock().await;
    let slash_evidence = state.slash_evidence.lock().await;
    let nonces = state.nonces.lock().await;

    let snapshot = AppSnapshot {
        chain: chain.clone(),
        token: token.clone(),
        contracts: contracts.clone(),
        pending_transactions: pending.clone(),
        stakers: stakers.clone(),
        peers: peers.clone(),
        governance: governance.clone(),
        slash_evidence: slash_evidence.clone(),
        nonces: nonces.clone(),
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

fn authorize_p2p(headers: &HeaderMap, state: &AppState) -> bool {
    // if no token configured, deny all requests
    let token = match state.p2p_token.as_ref() {
        Some(t) if !t.is_empty() => t,
        _ => return false, // no token set = deny
    };
    headers
        .get("x-p2p-token")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value == token)
}

fn authorize_admin(headers: &HeaderMap, state: &AppState) -> bool {
    let token = match state.admin_token.as_ref() {
        Some(t) if !t.is_empty() => t,
        _ => return false, // no token set = deny
    };
    headers
        .get("x-admin-token")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value == token)
}

/// Consume a per-account nonce. The nonce must be exactly the last used
/// nonce + 1, which makes every signed operation single-use.
async fn consume_nonce(state: &AppState, account: &str, nonce: u64) -> Result<(), String> {
    let mut nonces = state.nonces.lock().await;
    let expected = nonces.get(account).copied().unwrap_or(0) + 1;
    if nonce != expected {
        return Err(format!(
            "Invalid nonce for {}: expected {}, got {}",
            account, expected, nonce
        ));
    }
    nonces.insert(account.to_string(), nonce);
    Ok(())
}

pub fn api_routes() -> Router<AppState> {
    Router::new()
        // Certificate routes
        .route("/certificates/issue", post(issue_certificate))
        .route("/certificates/verify", post(verify_certificate))
        .route("/certificates/attest", post(attest_certificate))
        // Token routes
        .route("/tokens/transfer", post(transfer_tokens))
        .route("/tokens/faucet", post(faucet_tokens))
        .route("/tokens/balance/{account}", get(get_token_balance))
        .route("/tokens/nonce/{account}", get(get_account_nonce))
        // Blockchain routes
        .route("/blocks", get(get_blocks))
        .route("/blocks/{index}", get(get_block_by_index))
        .route("/blockchain/stats", get(get_blockchain_stats))
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

    // Create blockchain transaction anchoring the issuance
    let transaction = Transaction::new(
        None, // No sender for certificate issuance
        payload.issued_to.clone(),
        0, // Certificates don't transfer tokens
        TransactionType::Certificate,
    );

    // Add to pending transactions
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

/// Read-only certificate lookup: reports whether the certificate exists and
/// whether it has been attested. Does not mutate state.
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

// ===== TOKEN ENDPOINTS =====

async fn transfer_tokens(
    State(state): State<AppState>,
    Json(payload): Json<TokenTransferRequest>,
) -> Json<ApiResponse> {
    if payload.amount == 0 {
        return Json(ApiResponse {
            status: "error".to_string(),
            message: "Transfer amount must be greater than zero".to_string(),
        });
    }
    if payload.to.trim().is_empty() {
        return Json(ApiResponse {
            status: "error".to_string(),
            message: "Transfer recipient cannot be empty".to_string(),
        });
    }

    // Every transfer must be authorized by the sender's key — either a raw
    // secp256k1 signature (hikma-wallet) or an Ethereum personal_sign
    // signature (MetaMask).
    let signature = match &payload.signature {
        Some(value) => value.clone(),
        None => {
            return Json(ApiResponse {
                status: "error".to_string(),
                message: "Transfer requires a signature and nonce (plus public_key for \
                          raw secp256k1 signatures)"
                    .to_string(),
            });
        }
    };

    let message = Transaction::transfer_signing_message(
        &payload.from,
        &payload.to,
        payload.amount,
        payload.nonce,
    );
    if !crate::blockchain::transaction::verify_transfer_signature(
        &payload.from,
        &message,
        payload.public_key.as_deref(),
        &signature,
    ) {
        return Json(ApiResponse {
            status: "error".to_string(),
            message: "Transfer signature verification failed (sender must match the signing key)"
                .to_string(),
        });
    }

    // Consume the nonce before executing so a replayed request can never
    // execute twice.
    if let Err(err) = consume_nonce(&state, &payload.from, payload.nonce).await {
        return Json(ApiResponse {
            status: "error".to_string(),
            message: err,
        });
    }

    // Execute the balance change
    let mut token = state.token.lock().await;
    let success = token.transfer(&payload.from, &payload.to, payload.amount);
    drop(token);

    if success {
        // Anchor the signed transfer on-chain
        let mut transaction = Transaction::new(
            Some(payload.from.clone()),
            payload.to.clone(),
            payload.amount,
            TransactionType::Transfer,
        );
        transaction.nonce = payload.nonce;
        transaction.public_key = payload.public_key.clone();
        transaction.signature = Some(signature);

        let mut pending = state.pending_transactions.lock().await;
        pending.push(transaction);
        drop(pending);

        let _ = persist_state(&state).await;

        Json(ApiResponse {
            status: "success".to_string(),
            message: format!(
                "Transferred {} tokens from {} to {} and added to blockchain",
                payload.amount, payload.from, payload.to
            ),
        })
    } else {
        Json(ApiResponse {
            status: "error".to_string(),
            message: format!(
                "Failed to transfer tokens from {} to {} (insufficient balance)",
                payload.from, payload.to
            ),
        })
    }
}

/// Admin-gated faucet for funding accounts on dev/test networks.
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

    let mut token = state.token.lock().await;
    token.mint(&payload.to, payload.amount);
    drop(token);

    let _ = persist_state(&state).await;

    Json(ApiResponse {
        status: "success".to_string(),
        message: format!("Minted {} tokens to {}", payload.amount, payload.to),
    })
}

async fn get_token_balance(
    State(state): State<AppState>,
    Path(account): Path<String>,
) -> Json<BalanceResponse> {
    let token = state.token.lock().await;
    let balance = token.balance_of(&account);

    Json(BalanceResponse { account, balance })
}

#[derive(Serialize)]
pub struct NonceStateResponse {
    pub account: String,
    pub next_nonce: u64,
}

async fn get_account_nonce(
    State(state): State<AppState>,
    Path(account): Path<String>,
) -> Json<NonceStateResponse> {
    let nonces = state.nonces.lock().await;
    let next_nonce = nonces.get(&account).copied().unwrap_or(0) + 1;
    Json(NonceStateResponse {
        account,
        next_nonce,
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
        is_valid: chain.is_valid(),
        latest_hash: chain.latest_hash(),
        finalized_height: chain.finalized_height,
        finality_depth: governance.finality_depth,
    })
}

async fn get_explorer_overview(State(state): State<AppState>) -> Json<ExplorerOverview> {
    let chain = state.chain.lock().await;
    let pending = state.pending_transactions.lock().await;
    let peers = state.peers.lock().await;
    let stakers = state.stakers.lock().await;

    Json(ExplorerOverview {
        total_blocks: chain.blocks.len(),
        finalized_height: chain.finalized_height,
        pending_transactions: pending.len(),
        difficulty: chain.difficulty,
        latest_hash: chain.latest_hash(),
        peers: peers.len(),
        validators: stakers.len(),
        chain_valid: chain.is_valid(),
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
    staker_snapshot: Vec<Staker>,
    staker_set_hash: String,
    transactions: Vec<String>,
    included_ids: Vec<String>,
}

/// Select the validator for the next slot and assemble the block payload
/// (pending transactions plus the validator's reward transaction).
fn plan_block(
    chain: &Blockchain,
    stakers: &[Staker],
    pending: &[Transaction],
) -> Result<BlockPlan, String> {
    let has_only_genesis = chain.blocks.len() == 1;
    if pending.is_empty() && !has_only_genesis {
        return Err("No pending transactions to mine".to_string());
    }

    let next_index = chain.blocks.len() as u64;
    let seed = pos::selection_seed(&chain.latest_hash(), next_index);
    let validator = pos::select_staker_with_seed(&seed, stakers)
        .ok_or_else(|| "No validators available. Stake tokens to become a validator.".to_string())?;

    let staker_entry = stakers
        .iter()
        .find(|staker| staker.address == validator)
        .ok_or_else(|| "Selected validator not registered".to_string())?;
    let public_key = staker_entry
        .public_key
        .clone()
        .ok_or_else(|| "Validator missing public key".to_string())?;

    let staker_snapshot: Vec<Staker> = stakers.to_vec();
    let staker_set_hash = pos::staker_set_hash(&staker_snapshot);

    let mut transactions = Vec::with_capacity(pending.len() + 1);
    let mut included_ids = Vec::with_capacity(pending.len());
    for tx in pending {
        let serialized = serde_json::to_string(tx)
            .map_err(|err| format!("Failed to serialize transaction: {}", err))?;
        transactions.push(serialized);
        included_ids.push(tx.id.clone());
    }
    let reward = Transaction::new_reward(&validator);
    transactions.push(
        serde_json::to_string(&reward)
            .map_err(|err| format!("Failed to serialize reward transaction: {}", err))?,
    );

    Ok(BlockPlan {
        validator,
        public_key,
        staker_snapshot,
        staker_set_hash,
        transactions,
        included_ids,
    })
}

/// Apply local side effects of newly accepted blocks: mint the validator
/// reward and drop any pending transactions that are now on-chain.
async fn apply_accepted_blocks(state: &AppState, blocks: &[Block]) {
    let mut token = state.token.lock().await;
    for block in blocks {
        if let Some(validator) = &block.validator {
            token.mint(validator, BLOCK_REWARD);
        }
    }
    drop(token);

    let included_ids: HashSet<String> = blocks
        .iter()
        .flat_map(|block| block.transactions.iter())
        .filter_map(|tx_str| serde_json::from_str::<Transaction>(tx_str).ok())
        .map(|tx| tx.id)
        .collect();

    if !included_ids.is_empty() {
        let mut pending = state.pending_transactions.lock().await;
        pending.retain(|tx| !included_ids.contains(&tx.id));
    }
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
    let stakers = state.stakers.lock().await;

    let plan = match plan_block(&chain, &stakers, &pending) {
        Ok(plan) => plan,
        Err(message) => {
            drop(stakers);
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
        drop(stakers);
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
        drop(stakers);
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
        Some(plan.staker_set_hash),
        Some(plan.staker_snapshot),
    );

    let signature = match pos::sign_block_hash(&block.hash, &validator_key.private_key) {
        Ok(value) => value,
        Err(message) => {
            drop(stakers);
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

    if let Err(message) = chain.validate_block_candidate(&block) {
        drop(stakers);
        drop(chain);
        drop(pending);
        return Json(MiningResponse {
            status: "error".to_string(),
            message: format!("Mined block failed validation: {}", message),
            block_index: 0,
            transactions_count: 0,
        });
    }

    let accepted_block = block.clone();
    chain.add_mined_block(block);
    chain.apply_finality(finality_depth);
    let block_index = chain.blocks.len() as u64 - 1;

    // Included transactions leave the pending pool.
    let included: HashSet<String> = plan.included_ids.into_iter().collect();
    pending.retain(|tx| !included.contains(&tx.id));

    drop(stakers);
    drop(chain);
    drop(pending);

    // Reward the validator.
    {
        let mut token = state.token.lock().await;
        token.mint(&validator_key.address, BLOCK_REWARD);
    }

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
    let stakers = state.stakers.lock().await;

    let plan = match plan_block(&chain, &stakers, &pending) {
        Ok(plan) => plan,
        Err(message) => {
            return Json(ProposeBlockResponse {
                status: "error".to_string(),
                message,
                selected_validator: None,
                block: None,
                block_hash: None,
            });
        }
    };

    let block = chain.create_block(
        plan.transactions,
        Some(plan.validator.clone()),
        Some(plan.public_key),
        Some(plan.staker_set_hash),
        Some(plan.staker_snapshot),
    );

    let block_hash = block.hash.clone();
    Json(ProposeBlockResponse {
        status: "success".to_string(),
        message: format!(
            "Block candidate created for validator {}. Sign block_hash with the validator's \
             private key (hikma-wallet sign-block) and POST the block with validator_signature \
             set to /mine/submit.",
            plan.validator
        ),
        selected_validator: Some(plan.validator),
        block: Some(block),
        block_hash: Some(block_hash),
    })
}

/// Accept a fully signed block produced via /mine/propose (or by any
/// validator client). The block passes full consensus validation.
async fn submit_block(
    State(state): State<AppState>,
    Json(block): Json<Block>,
) -> Json<MiningResponse> {
    let finality_depth = {
        let governance = state.governance.lock().await;
        governance.finality_depth
    };

    let mut chain = state.chain.lock().await;
    if let Err(message) = chain.validate_block_candidate(&block) {
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

    let accepted_block = block.clone();
    let transactions_count = block.transactions.len();
    chain.add_mined_block(block);
    chain.apply_finality(finality_depth);
    let block_index = chain.blocks.len() as u64 - 1;
    drop(chain);

    apply_accepted_blocks(&state, std::slice::from_ref(&accepted_block)).await;

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

    let public_key = match &payload.public_key {
        Some(value) => value.clone(),
        None => {
            return Json(StakeResponse {
                status: "error".to_string(),
                message: "Validator registration requires a public_key. \
                          Private keys must never be sent to the server; \
                          sign the stake request locally."
                    .to_string(),
                total_stake: 0,
            });
        }
    };
    let signature = match &payload.signature {
        Some(value) => value.clone(),
        None => {
            return Json(StakeResponse {
                status: "error".to_string(),
                message: "Staking requires a signature over \
                          hikmalayer-stake:{address}:{amount}:{nonce}"
                    .to_string(),
                total_stake: 0,
            });
        }
    };

    // The staking address is bound to the key that signs for it.
    let derived = match pos::derive_address(&public_key) {
        Ok(value) => value,
        Err(err) => {
            return Json(StakeResponse {
                status: "error".to_string(),
                message: format!("Invalid public key: {}", err),
                total_stake: 0,
            });
        }
    };
    if derived.to_lowercase() != payload.address.to_lowercase() {
        return Json(StakeResponse {
            status: "error".to_string(),
            message: "Staking address must be the address derived from public_key".to_string(),
            total_stake: 0,
        });
    }

    let message = format!(
        "hikmalayer-stake:{}:{}:{}",
        payload.address, payload.amount, payload.nonce
    );
    if !pos::verify_message(&message, &public_key, &signature) {
        return Json(StakeResponse {
            status: "error".to_string(),
            message: "Stake signature verification failed".to_string(),
            total_stake: 0,
        });
    }

    if let Err(err) = consume_nonce(&state, &payload.address, payload.nonce).await {
        return Json(StakeResponse {
            status: "error".to_string(),
            message: err,
            total_stake: 0,
        });
    }

    let mut token = state.token.lock().await;
    let transfer_success = token.transfer(&payload.address, STAKING_POOL_ACCOUNT, payload.amount);
    drop(token);

    if !transfer_success {
        return Json(StakeResponse {
            status: "error".to_string(),
            message: format!("Insufficient balance to stake for {}", payload.address),
            total_stake: 0,
        });
    }

    let mut stakers = state.stakers.lock().await;
    let mut total_stake = 0;
    let mut found = false;

    for staker in stakers.iter_mut() {
        if staker.address == payload.address {
            staker.stake += payload.amount;
            staker.public_key = Some(public_key.clone());
            found = true;
        }
        total_stake += staker.stake;
    }

    if !found {
        stakers.push(Staker {
            address: payload.address.clone(),
            stake: payload.amount,
            public_key: Some(public_key.clone()),
        });
        total_stake += payload.amount;
    }

    drop(stakers);
    let _ = persist_state(&state).await;

    Json(StakeResponse {
        status: "success".to_string(),
        message: format!("Staked {} tokens for {}", payload.amount, payload.address),
        total_stake,
    })
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

    let signature = match &payload.signature {
        Some(value) => value.clone(),
        None => {
            return Json(StakeResponse {
                status: "error".to_string(),
                message: "Withdrawal requires a signature over \
                          hikmalayer-withdraw:{address}:{amount}:{nonce}"
                    .to_string(),
                total_stake: 0,
            });
        }
    };

    let mut stakers = state.stakers.lock().await;
    let mut total_stake = 0;
    let mut available_stake = None;
    let mut registered_key = None;

    for staker in stakers.iter() {
        total_stake += staker.stake;
        if staker.address == payload.address {
            available_stake = Some(staker.stake);
            registered_key = staker.public_key.clone();
        }
    }

    let available_stake = match available_stake {
        Some(stake) => stake,
        None => {
            return Json(StakeResponse {
                status: "error".to_string(),
                message: format!("No stake found for {}", payload.address),
                total_stake,
            });
        }
    };

    // Withdrawals are authorized by the validator's registered key only.
    let registered_key = match registered_key {
        Some(key) => key,
        None => {
            return Json(StakeResponse {
                status: "error".to_string(),
                message: format!("No registered public key for {}", payload.address),
                total_stake,
            });
        }
    };

    let message = format!(
        "hikmalayer-withdraw:{}:{}:{}",
        payload.address, payload.amount, payload.nonce
    );
    if !pos::verify_message(&message, &registered_key, &signature) {
        return Json(StakeResponse {
            status: "error".to_string(),
            message: "Withdrawal signature verification failed".to_string(),
            total_stake,
        });
    }

    if available_stake < payload.amount {
        return Json(StakeResponse {
            status: "error".to_string(),
            message: format!("Insufficient staked balance for {}", payload.address),
            total_stake,
        });
    }

    if let Err(err) = consume_nonce(&state, &payload.address, payload.nonce).await {
        return Json(StakeResponse {
            status: "error".to_string(),
            message: err,
            total_stake,
        });
    }

    let mut token = state.token.lock().await;
    let transfer_success = token.transfer(STAKING_POOL_ACCOUNT, &payload.address, payload.amount);
    drop(token);

    if !transfer_success {
        return Json(StakeResponse {
            status: "error".to_string(),
            message: "Staking pool has insufficient balance".to_string(),
            total_stake,
        });
    }

    let mut updated_total = 0;
    stakers.retain(|staker| {
        if staker.address == payload.address {
            let remaining = staker.stake.saturating_sub(payload.amount);
            if remaining > 0 {
                updated_total += remaining;
                return true;
            }
            return false;
        }
        updated_total += staker.stake;
        true
    });
    drop(stakers);

    let _ = persist_state(&state).await;

    Json(StakeResponse {
        status: "success".to_string(),
        message: format!(
            "Withdrew {} staked tokens for {}",
            payload.amount, payload.address
        ),
        total_stake: updated_total,
    })
}

async fn list_validators(State(state): State<AppState>) -> Json<Vec<ValidatorInfo>> {
    let stakers = state.stakers.lock().await;
    let validators = stakers
        .iter()
        .map(|staker| ValidatorInfo {
            address: staker.address.clone(),
            stake: staker.stake,
            public_key: staker.public_key.clone(),
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

    if let Err(message) = chain.validate_block_candidate(&block) {
        drop(chain);
        if message.contains("chain tip") {
            let state_clone = state.clone();
            tokio::spawn(async move {
                sync_with_peers(state_clone).await;
            });
        }
        return Err(message);
    }

    let accepted = block.clone();
    chain.add_mined_block(block);
    chain.apply_finality(finality_depth);
    let height = chain.blocks.len() as u64 - 1;
    drop(chain);

    apply_accepted_blocks(state, std::slice::from_ref(&accepted)).await;
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

    let mut stakers = state.stakers.lock().await;
    let governance = state.governance.lock().await;
    let slashed_amount =
        pos::slash_staker_with_percent(&mut stakers, &evidence.validator, governance.slash_percent);
    drop(stakers);
    drop(governance);

    let mut metrics = state.metrics.lock().await;
    metrics.slashes_submitted += 1;
    drop(metrics);

    let mut evidence_log = state.slash_evidence.lock().await;
    evidence_log.push(crate::persistence::SlashEvidence {
        block_index: payload.block_index,
        reason: evidence.reason,
        reporter: payload.reporter,
        timestamp: evidence.timestamp,
        slashed_amount,
    });
    drop(evidence_log);
    let _ = persist_state(&state).await;

    Json(SlashEvidenceResponse {
        status: "success".to_string(),
        message: format!(
            "Slashed validator {} by {}",
            evidence.validator, slashed_amount
        ),
        slashed_amount,
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
    let mut stakers = state.stakers.lock().await;
    let (is_valid, slashed, details) = chain.validate_and_slash(&mut stakers);

    drop(stakers);
    drop(chain);
    let _ = persist_state(&state).await;

    Json(ValidationResponse {
        is_valid,
        message: if is_valid {
            "Blockchain is valid".to_string()
        } else {
            "Blockchain validation failed".to_string()
        },
        details: if is_valid {
            Some("All blocks properly linked with valid PoW and PoS data".to_string())
        } else {
            details
        },
        slashed: slashed
            .into_iter()
            .map(|(address, amount)| SlashEvent { address, amount })
            .collect(),
    })
}

async fn validate_block(
    State(state): State<AppState>,
    Path(index): Path<usize>,
) -> Json<ValidationResponse> {
    let chain = state.chain.lock().await;
    let mut stakers = state.stakers.lock().await;

    if index >= chain.blocks.len() {
        return Json(ValidationResponse {
            is_valid: false,
            message: "Block not found".to_string(),
            details: Some(format!("Block index {} does not exist", index)),
            slashed: Vec::new(),
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
            slashed: Vec::new(),
        });
    }

    let mut slashed = Vec::new();
    let result = chain.evaluate_slash_evidence(index as u64);

    let (is_valid, error) = match result {
        // evaluate_slash_evidence returns Ok(evidence) when misbehavior found
        Ok(evidence) => {
            let amount = pos::slash_staker(&mut stakers, &evidence.validator);
            if amount > 0 {
                slashed.push(SlashEvent {
                    address: evidence.validator.clone(),
                    amount,
                });
            }
            (false, Some(evidence.reason))
        }
        Err(message) if message.contains("does not contain slashable") => (true, None),
        Err(message) => (false, Some(message)),
    };

    drop(stakers);
    drop(chain);
    let _ = persist_state(&state).await;

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
        slashed,
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
    use axum::http::HeaderValue;

    const ADMIN_TOKEN: &str = "test-admin-token";
    const P2P_TOKEN: &str = "test-p2p-token";

    fn test_state(validator_key: Option<LocalValidatorKey>) -> AppState {
        std::env::set_var(
            "HIKMALAYER_STATE_PATH",
            format!(
                "{}/hikmalayer-test-state-{}.json",
                std::env::temp_dir().display(),
                uuid::Uuid::new_v4()
            ),
        );
        AppState {
            chain: Arc::new(Mutex::new(Blockchain::new(2))),
            token: Arc::new(Mutex::new(Token::new("Test", "TST", 1000, "admin"))),
            contracts: Arc::new(Mutex::new(ContractExecutor::new())),
            pending_transactions: Arc::new(Mutex::new(Vec::new())),
            auth_manager: Arc::new(Mutex::new(AuthManager::new())),
            stakers: Arc::new(Mutex::new(Vec::new())),
            peers: Arc::new(Mutex::new(Vec::new())),
            governance: Arc::new(Mutex::new(GovernanceConfig::default())),
            slash_evidence: Arc::new(Mutex::new(Vec::new())),
            metrics: Arc::new(Mutex::new(Metrics::default())),
            nonces: Arc::new(Mutex::new(HashMap::new())),
            seen_messages: Arc::new(Mutex::new(SeenMessageCache::new(1024))),
            p2p_token: Some(P2P_TOKEN.to_string()),
            admin_token: Some(ADMIN_TOKEN.to_string()),
            p2p_service: Arc::new(P2PService::new("node-test".to_string(), None).unwrap()),
            validator_key,
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

    async fn fund(state: &AppState, account: &str, amount: u64) {
        let mut token = state.token.lock().await;
        token.mint(account, amount);
    }

    async fn register_staker(state: &AppState, seed: u8, stake: u64) -> (String, String, String) {
        let (address, public_key, private_key) = test_wallet(seed);
        fund(state, &address, stake * 2).await;

        let message = format!("hikmalayer-stake:{}:{}:{}", address, stake, 1);
        let signature = pos::sign_message(&message, &private_key).unwrap();
        let response = stake_tokens(
            State(state.clone()),
            Json(StakeRequest {
                address: address.clone(),
                amount: stake,
                public_key: Some(public_key.clone()),
                nonce: 1,
                signature: Some(signature),
            }),
        )
        .await;
        assert_eq!(response.0.status, "success", "{}", response.0.message);
        (address, public_key, private_key)
    }

    #[tokio::test]
    async fn transfer_without_signature_is_rejected() {
        let state = test_state(None);
        let response = transfer_tokens(
            State(state),
            Json(TokenTransferRequest {
                from: "admin".to_string(),
                to: "0xabc".to_string(),
                amount: 10,
                nonce: 1,
                public_key: None,
                signature: None,
            }),
        )
        .await;
        assert_eq!(response.0.status, "error");
        assert!(response.0.message.contains("signature"));
    }

    #[tokio::test]
    async fn signed_transfer_succeeds_and_replay_is_rejected() {
        let state = test_state(None);
        let (address, public_key, private_key) = test_wallet(11);
        fund(&state, &address, 100).await;

        let message = Transaction::transfer_signing_message(&address, "0xrecipient", 40, 1);
        let signature = pos::sign_message(&message, &private_key).unwrap();
        let request = || TokenTransferRequest {
            from: address.clone(),
            to: "0xrecipient".to_string(),
            amount: 40,
            nonce: 1,
            public_key: Some(public_key.clone()),
            signature: Some(signature.clone()),
        };

        let response = transfer_tokens(State(state.clone()), Json(request())).await;
        assert_eq!(response.0.status, "success", "{}", response.0.message);
        assert_eq!(state.token.lock().await.balance_of("0xrecipient"), 40);

        // Replaying the exact same signed request must fail on the nonce.
        let replay = transfer_tokens(State(state.clone()), Json(request())).await;
        assert_eq!(replay.0.status, "error");
        assert!(replay.0.message.contains("nonce"));
        assert_eq!(state.token.lock().await.balance_of("0xrecipient"), 40);
    }

    #[tokio::test]
    async fn transfer_with_wrong_sender_binding_is_rejected() {
        let state = test_state(None);
        let (_, public_key, private_key) = test_wallet(12);
        let (victim, _, _) = test_wallet(13);
        fund(&state, &victim, 100).await;

        // Attacker signs correctly with their own key but claims the
        // victim's account as sender.
        let message = Transaction::transfer_signing_message(&victim, "0xattacker", 40, 1);
        let signature = pos::sign_message(&message, &private_key).unwrap();
        let response = transfer_tokens(
            State(state.clone()),
            Json(TokenTransferRequest {
                from: victim.clone(),
                to: "0xattacker".to_string(),
                amount: 40,
                nonce: 1,
                public_key: Some(public_key),
                signature: Some(signature),
            }),
        )
        .await;
        assert_eq!(response.0.status, "error");
        assert_eq!(state.token.lock().await.balance_of(&victim), 100);
    }

    #[tokio::test]
    async fn stake_requires_address_key_binding() {
        let state = test_state(None);
        let (_, public_key, private_key) = test_wallet(14);
        fund(&state, "0xnot-my-address", 100).await;

        let message = format!("hikmalayer-stake:{}:{}:{}", "0xnot-my-address", 50, 1);
        let signature = pos::sign_message(&message, &private_key).unwrap();
        let response = stake_tokens(
            State(state),
            Json(StakeRequest {
                address: "0xnot-my-address".to_string(),
                amount: 50,
                public_key: Some(public_key),
                nonce: 1,
                signature: Some(signature),
            }),
        )
        .await;
        assert_eq!(response.0.status, "error");
        assert!(response.0.message.contains("derived"));
    }

    #[tokio::test]
    async fn withdraw_requires_registered_key_signature() {
        let state = test_state(None);
        let (address, _, private_key) = register_staker(&state, 15, 50).await;

        // Unsigned withdrawal fails.
        let response = withdraw_stake(
            State(state.clone()),
            Json(StakeRequest {
                address: address.clone(),
                amount: 20,
                public_key: None,
                nonce: 2,
                signature: None,
            }),
        )
        .await;
        assert_eq!(response.0.status, "error");

        // Properly signed withdrawal succeeds.
        let message = format!("hikmalayer-withdraw:{}:{}:{}", address, 20, 2);
        let signature = pos::sign_message(&message, &private_key).unwrap();
        let response = withdraw_stake(
            State(state.clone()),
            Json(StakeRequest {
                address: address.clone(),
                amount: 20,
                public_key: None,
                nonce: 2,
                signature: Some(signature),
            }),
        )
        .await;
        assert_eq!(response.0.status, "success", "{}", response.0.message);
    }

    #[tokio::test]
    async fn mine_with_local_validator_key_end_to_end() {
        let state = test_state(None);
        let (address, _, private_key) = register_staker(&state, 16, 100).await;

        // Wire the local validator identity after registration.
        let mut state = state;
        state.validator_key = Some(LocalValidatorKey::from_private_key(&private_key).unwrap());

        let balance_before = state.token.lock().await.balance_of(&address);
        let response = mine_block(State(state.clone())).await;
        assert_eq!(response.0.status, "success", "{}", response.0.message);
        assert_eq!(response.0.block_index, 1);

        let chain = state.chain.lock().await;
        assert_eq!(chain.blocks.len(), 2);
        assert!(chain.is_valid());
        drop(chain);

        // Block reward was minted.
        let balance_after = state.token.lock().await.balance_of(&address);
        assert_eq!(balance_after, balance_before + BLOCK_REWARD);
    }

    #[tokio::test]
    async fn propose_and_submit_flow_produces_valid_block() {
        let state = test_state(None);
        let (_, _, private_key) = register_staker(&state, 17, 100).await;

        let proposal = propose_block(State(state.clone())).await;
        assert_eq!(proposal.0.status, "success", "{}", proposal.0.message);
        let mut block = proposal.0.block.unwrap();
        let block_hash = proposal.0.block_hash.unwrap();

        // Validator signs the proposal offline.
        block.validator_signature =
            Some(pos::sign_block_hash(&block_hash, &private_key).unwrap());

        let response = submit_block(State(state.clone()), Json(block)).await;
        assert_eq!(response.0.status, "success", "{}", response.0.message);

        let chain = state.chain.lock().await;
        assert_eq!(chain.blocks.len(), 2);
        assert!(chain.is_valid());
    }

    #[tokio::test]
    async fn submit_rejects_unsigned_or_forged_blocks() {
        let state = test_state(None);
        register_staker(&state, 18, 100).await;

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
    async fn difficulty_change_requires_admin_and_bounds() {
        let state = test_state(None);

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
    async fn faucet_and_certificates_require_admin() {
        let state = test_state(None);

        let response = faucet_tokens(
            State(state.clone()),
            HeaderMap::new(),
            Json(FaucetRequest {
                to: "0xuser".to_string(),
                amount: 10,
            }),
        )
        .await;
        assert_eq!(response.0.status, "error");

        let response = faucet_tokens(
            State(state.clone()),
            admin_headers(),
            Json(FaucetRequest {
                to: "0xuser".to_string(),
                amount: 10,
            }),
        )
        .await;
        assert_eq!(response.0.status, "success");
        assert_eq!(state.token.lock().await.balance_of("0xuser"), 10);

        let response = issue_certificate(
            State(state.clone()),
            HeaderMap::new(),
            Json(CertificateRequest {
                id: "cert-1".to_string(),
                issued_to: "0xuser".to_string(),
                description: "test".to_string(),
            }),
        )
        .await;
        assert_eq!(response.0.status, "error");
    }

    #[tokio::test]
    async fn p2p_endpoints_reject_missing_token_and_replays() {
        let state = test_state(None);

        // Unauthorized block push.
        let block = state.chain.lock().await.blocks[0].clone();
        let response = receive_block(State(state.clone()), HeaderMap::new(), Json(block)).await;
        assert_eq!(response.0.status, "error");

        // Duplicate protocol envelope is rejected on the second delivery.
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
    async fn peer_block_acceptance_pays_reward_and_clears_pending() {
        let state = test_state(None);
        let (address, _, private_key) = register_staker(&state, 19, 100).await;

        // Build a signed block via propose.
        let proposal = propose_block(State(state.clone())).await;
        let mut block = proposal.0.block.unwrap();
        block.validator_signature =
            Some(pos::sign_block_hash(&block.hash, &private_key).unwrap());

        let balance_before = state.token.lock().await.balance_of(&address);
        let response = receive_block(State(state.clone()), p2p_headers(), Json(block)).await;
        assert_eq!(response.0.status, "success", "{}", response.0.message);

        let balance_after = state.token.lock().await.balance_of(&address);
        assert_eq!(balance_after, balance_before + BLOCK_REWARD);
        assert_eq!(state.chain.lock().await.blocks.len(), 2);
    }
}
