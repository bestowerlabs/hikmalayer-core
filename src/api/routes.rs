use axum::{
    extract::{Path, Query, State},
    http::HeaderMap,
    response::Json,
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::{
    auth::AuthManager,
    blockchain::{
        block::Block,
        chain::Blockchain,
        transaction::{Transaction, TransactionType},
    },
    consensus::pos::{self, Staker},
    contract::contract::ContractExecutor,
    governance::GovernanceConfig,
    p2p::{
        protocol::{P2PEnvelope, P2PPayload},
        service::P2PService,
    },
    persistence::{save_state, AppSnapshot},
    token::fungible::Token,
};

const STAKING_POOL_ACCOUNT: &str = "__staking_pool__";

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
    pub p2p_token: Option<String>,
    pub admin_token: Option<String>,
    pub p2p_service: Arc<P2PService>,
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
    pub private_key: Option<String>,
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

    let stakers_snapshot: Vec<Staker> = stakers
        .iter()
        .map(|staker| Staker {
            address: staker.address.clone(),
            stake: staker.stake,
            public_key: staker.public_key.clone(),
            private_key: None,
        })
        .collect();

    let snapshot = AppSnapshot {
        chain: chain.clone(),
        token: token.clone(),
        contracts: contracts.clone(),
        pending_transactions: pending.clone(),
        stakers: stakers_snapshot,
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

fn authorize_p2p(headers: &HeaderMap, state: &AppState) -> bool {
    let Some(token) = state.p2p_token.as_ref() else {
        return true;
    };
    headers
        .get("x-p2p-token")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value == token)
}

fn authorize_admin(headers: &HeaderMap, state: &AppState) -> bool {
    let Some(token) = state.admin_token.as_ref() else {
        return true;
    };
    headers
        .get("x-admin-token")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value == token)
}

pub fn api_routes() -> Router<AppState> {
    Router::new()
        // Certificate routes
        .route("/certificates/issue", post(issue_certificate))
        .route("/certificates/verify", post(verify_certificate))
        // Token routes
        .route("/tokens/transfer", post(transfer_tokens))
        .route("/tokens/balance/{account}", get(get_token_balance))
        // Blockchain routes
        .route("/blocks", get(get_blocks))
        .route("/blocks/{index}", get(get_block_by_index))
        .route("/blockchain/stats", get(get_blockchain_stats))
        // Mining routes
        .route("/mine", post(mine_block))
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
    Json(payload): Json<CertificateRequest>,
) -> Json<ApiResponse> {
    // Update contract state
    let mut contracts = state.contracts.lock().await;
    contracts.issue_certificate(&payload.id, &payload.issued_to, &payload.description);
    drop(contracts);

    // Create blockchain transaction
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

async fn verify_certificate(
    State(state): State<AppState>,
    Json(payload): Json<VerifyCertificateRequest>,
) -> Json<ApiResponse> {
    let mut contracts = state.contracts.lock().await;
    let success = contracts.verify_certificate(&payload.id);

    let _ = persist_state(&state).await;

    Json(ApiResponse {
        status: if success { "success" } else { "error" }.to_string(),
        message: if success {
            format!("Certificate {} verified", payload.id)
        } else {
            format!("Failed to verify certificate {}", payload.id)
        },
    })
}

// ===== TOKEN ENDPOINTS =====

async fn transfer_tokens(
    State(state): State<AppState>,
    Json(payload): Json<TokenTransferRequest>,
) -> Json<ApiResponse> {
    // Update token balances
    let mut token = state.token.lock().await;
    let success = token.transfer(&payload.from, &payload.to, payload.amount);
    drop(token);

    if success {
        // Create blockchain transaction
        let transaction = Transaction::new(
            Some(payload.from.clone()),
            payload.to.clone(),
            payload.amount,
            TransactionType::Transfer,
        );

        // Add to pending transactions
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
                "Failed to transfer tokens from {} to {}",
                payload.from, payload.to
            ),
        })
    }
}

async fn get_token_balance(
    State(state): State<AppState>,
    Path(account): Path<String>,
) -> Json<BalanceResponse> {
    let token = state.token.lock().await;
    let balance = token.balance_of(&account);

    Json(BalanceResponse { account, balance })
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

async fn mine_block(State(state): State<AppState>) -> Json<MiningResponse> {
    let finality_depth = {
        let governance = state.governance.lock().await;
        governance.finality_depth
    };
    let mut pending = state.pending_transactions.lock().await;
    let mut chain = state.chain.lock().await;
    let stakers = state.stakers.lock().await;

    // Since genesis block is auto-created, we only need to check for pending transactions
    // Allow mining if there are pending transactions OR if there's only the genesis block
    let has_only_genesis = chain.blocks.len() == 1;

    if pending.is_empty() && !has_only_genesis {
        drop(chain);
        drop(pending);
        drop(stakers);
        return Json(MiningResponse {
            status: "info".to_string(),
            message: "No pending transactions to mine".to_string(),
            block_index: 0,
            transactions_count: 0,
        });
    }

    let seed = chain.latest_hash();
    let validator = pos::select_staker_with_seed(&seed, &stakers);
    if validator.is_none() {
        drop(chain);
        drop(pending);
        drop(stakers);
        return Json(MiningResponse {
            status: "error".to_string(),
            message: "No validators available. Stake tokens to become a validator.".to_string(),
            block_index: 0,
            transactions_count: 0,
        });
    }

    let validator = validator.unwrap();
    let staker_entry = match stakers.iter().find(|staker| staker.address == validator) {
        Some(value) => value,
        None => {
            drop(chain);
            drop(pending);
            drop(stakers);
            return Json(MiningResponse {
                status: "error".to_string(),
                message: "Selected validator not registered".to_string(),
                block_index: 0,
                transactions_count: 0,
            });
        }
    };
    let public_key = match &staker_entry.public_key {
        Some(value) => value.clone(),
        None => {
            drop(chain);
            drop(pending);
            drop(stakers);
            return Json(MiningResponse {
                status: "error".to_string(),
                message: "Validator missing public key".to_string(),
                block_index: 0,
                transactions_count: 0,
            });
        }
    };
    let private_key = match &staker_entry.private_key {
        Some(value) => value.clone(),
        None => {
            drop(chain);
            drop(pending);
            drop(stakers);
            return Json(MiningResponse {
                status: "error".to_string(),
                message: "Validator missing private key".to_string(),
                block_index: 0,
                transactions_count: 0,
            });
        }
    };
    let staker_snapshot: Vec<Staker> = stakers
        .iter()
        .map(|staker| Staker {
            address: staker.address.clone(),
            stake: staker.stake,
            public_key: staker.public_key.clone(),
            private_key: None,
        })
        .collect();
    let staker_set_hash = pos::staker_set_hash(&staker_snapshot);

    let transactions_count: usize;
    let transaction_strings: Vec<String>;

    if has_only_genesis && pending.is_empty() {
        // For the first user-initiated mining after genesis, create a welcome transaction
        transaction_strings = vec![
            "First mined block - Blockchain is now active!".to_string(),
            format!("Validator: {}", validator),
        ];
        transactions_count = transaction_strings.len();
    } else {
        // Convert pending transactions to strings for the block
        transaction_strings = pending.iter().map(|tx| format!("{:?}", tx)).collect();
        transactions_count = transaction_strings.len();

        // Clear pending transactions after copying them
        pending.clear();
    }

    let mut block = chain.create_block(
        transaction_strings,
        Some(validator.clone()),
        Some(public_key),
        Some(staker_set_hash),
        Some(staker_snapshot),
    );
    let signature = match pos::sign_block_hash(&block.hash, &private_key) {
        Ok(value) => value,
        Err(message) => {
            drop(chain);
            drop(pending);
            drop(stakers);
            return Json(MiningResponse {
                status: "error".to_string(),
                message: format!("Failed to sign block: {}", message),
                block_index: 0,
                transactions_count: 0,
            });
        }
    };
    block.validator_signature = Some(signature);
    chain.add_mined_block(block);
    chain.apply_finality(finality_depth);
    let block_index = chain.blocks.len() as u64 - 1;
    let block_to_gossip = chain.blocks.last().cloned();

    // Release locks
    drop(chain);
    drop(pending);
    drop(stakers);

    let mut metrics = state.metrics.lock().await;
    metrics.blocks_mined += 1;
    drop(metrics);

    let _ = persist_state(&state).await;
    if let Some(block) = block_to_gossip {
        let state_clone = state.clone();
        tokio::spawn(async move {
            let _ = gossip_blocks(&state_clone, vec![block]).await;
        });
    }

    Json(MiningResponse {
        status: "success".to_string(),
        message: if has_only_genesis {
            format!(
                "Successfully mined the first block! 🎉 Validator {} secured the block.",
                validator
            )
        } else {
            format!(
                "Successfully mined block with {} transactions. Validator: {}",
                transactions_count, validator
            )
        },
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
    Json(payload): Json<DifficultyRequest>,
) -> Json<ApiResponse> {
    let mut chain = state.chain.lock().await;
    let old_difficulty = chain.difficulty;
    chain.difficulty = payload.difficulty;

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
            if payload.public_key.is_some() {
                staker.public_key = payload.public_key.clone();
            }
            if payload.private_key.is_some() {
                staker.private_key = payload.private_key.clone();
            }
            found = true;
        }
        total_stake += staker.stake;
    }

    if !found {
        if payload.public_key.is_none() || payload.private_key.is_none() {
            return Json(StakeResponse {
                status: "error".to_string(),
                message: "Validator registration requires public_key and private_key".to_string(),
                total_stake,
            });
        }
        stakers.push(Staker {
            address: payload.address.clone(),
            stake: payload.amount,
            public_key: payload.public_key.clone(),
            private_key: payload.private_key.clone(),
        });
        total_stake += payload.amount;
    }

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

    let mut stakers = state.stakers.lock().await;
    let mut total_stake = 0;
    let mut available_stake = None;

    for staker in stakers.iter() {
        total_stake += staker.stake;
        if staker.address == payload.address {
            available_stake = Some(staker.stake);
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

    if available_stake < payload.amount {
        return Json(StakeResponse {
            status: "error".to_string(),
            message: format!("Insufficient staked balance for {}", payload.address),
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
    let mut chain = state.chain.lock().await;

    if let Err(message) = chain.validate_block_candidate(&block) {
        return Json(ApiResponse {
            status: "error".to_string(),
            message,
        });
    }

    chain.add_mined_block(block);
    chain.apply_finality(finality_depth);
    drop(chain);
    let mut metrics = state.metrics.lock().await;
    metrics.blocks_received += 1;
    let _ = persist_state(&state).await;

    Json(ApiResponse {
        status: "success".to_string(),
        message: "Block accepted".to_string(),
    })
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
    let mut chain = state.chain.lock().await;
    let mut accepted = 0u64;

    for block in blocks {
        if chain.validate_block_candidate(&block).is_ok() {
            chain.add_mined_block(block);
            accepted += 1;
        } else {
            break;
        }
    }
    if accepted > 0 {
        chain.apply_finality(finality_depth);
    }
    drop(chain);

    if accepted > 0 {
        let mut metrics = state.metrics.lock().await;
        metrics.blocks_received += accepted;
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
            let mut chain = state.chain.lock().await;
            if let Err(message) = chain.validate_block_candidate(&block) {
                let mut metrics = state.metrics.lock().await;
                metrics.protocol_messages_rejected += 1;
                return Json(ApiResponse {
                    status: "error".to_string(),
                    message,
                });
            }
            chain.add_mined_block(block);
            chain.apply_finality(finality_depth);
            drop(chain);
            let mut metrics = state.metrics.lock().await;
            metrics.blocks_received += 1;
            drop(metrics);
            let _ = persist_state(&state).await;
            Json(ApiResponse {
                status: "success".to_string(),
                message: "Block accepted".to_string(),
            })
        }
        P2PPayload::BlockBatch(blocks) => {
            let finality_depth = {
                let governance = state.governance.lock().await;
                governance.finality_depth
            };
            let mut chain = state.chain.lock().await;
            let mut accepted = 0u64;
            for block in blocks {
                if chain.validate_block_candidate(&block).is_ok() {
                    chain.add_mined_block(block);
                    accepted += 1;
                } else {
                    break;
                }
            }
            if accepted > 0 {
                chain.apply_finality(finality_depth);
            }
            drop(chain);
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
        // Genesis block is always valid
        return Json(ValidationResponse {
            is_valid: true,
            message: "Genesis block is valid".to_string(),
            details: Some("Genesis block validation passed".to_string()),
            slashed: Vec::new(),
        });
    }

    let current_block = &chain.blocks[index];
    let previous_block = &chain.blocks[index - 1];
    let mut slashed = Vec::new();
    let mut error = None;

    if current_block.previous_hash != previous_block.hash {
        error = Some("Previous hash does not match".to_string());
    } else if current_block.validator.is_none() {
        error = Some("Missing validator".to_string());
    } else if current_block.validator_public_key.is_none() {
        error = Some("Missing validator public key".to_string());
    } else if current_block.validator_signature.is_none() {
        error = Some("Missing validator signature".to_string());
    } else if current_block.staker_snapshot.is_none() || current_block.staker_set_hash.is_none() {
        error = Some("Missing staker snapshot data".to_string());
    } else {
        let staker_snapshot = current_block.staker_snapshot.as_ref().unwrap();
        let staker_hash = pos::staker_set_hash(staker_snapshot);
        if Some(staker_hash) != current_block.staker_set_hash {
            error = Some("Staker set hash mismatch".to_string());
        } else {
            let expected =
                pos::select_staker_with_seed(&current_block.previous_hash, staker_snapshot);
            if expected != current_block.validator {
                if let Some(validator) = &current_block.validator {
                    let amount = pos::slash_staker(&mut stakers, validator);
                    if amount > 0 {
                        slashed.push(SlashEvent {
                            address: validator.clone(),
                            amount,
                        });
                    }
                }
                error = Some("Validator does not match PoS selection".to_string());
            } else if !pos::verify_block_signature(
                &current_block.hash,
                current_block.validator_public_key.as_ref().unwrap(),
                current_block.validator_signature.as_ref().unwrap(),
            ) {
                if let Some(validator) = &current_block.validator {
                    let amount = pos::slash_staker(&mut stakers, validator);
                    if amount > 0 {
                        slashed.push(SlashEvent {
                            address: validator.clone(),
                            amount,
                        });
                    }
                }
                error = Some("Block failed signature verification".to_string());
            } else if !current_block.has_valid_pow() {
                if let Some(validator) = &current_block.validator {
                    let amount = pos::slash_staker(&mut stakers, validator);
                    if amount > 0 {
                        slashed.push(SlashEvent {
                            address: validator.clone(),
                            amount,
                        });
                    }
                }
                error = Some("Block failed PoW validation".to_string());
            }
        }
    }

    let is_valid = error.is_none();

    drop(stakers);
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
