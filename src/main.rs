use axum::http::Method;
use hikmalayer::api::routes::{api_routes, AppState, LocalValidatorKey, Metrics};
use hikmalayer::auth::{routes::auth_routes, AuthManager};
use hikmalayer::blockchain::chain::{Blockchain, DEFAULT_GENESIS_SUPPLY};
use hikmalayer::consensus::pos;
use hikmalayer::contract::contract::ContractExecutor;
use hikmalayer::p2p::{peerbook::PeerBook, protocol::SeenMessageCache, service::P2PService};
use hikmalayer::persistence::load_state;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use tower_http::cors::{Any, CorsLayer};

/// Fast-sync a fresh node from a checkpoint bundle. When `HIKMALAYER_CHECKPOINT`
/// names a JSON `CheckpointBundle` (as served by `/checkpoint/bundle`), the node
/// boots from a weak-subjectivity anchor sitting on a retarget boundary instead
/// of replaying the full chain from genesis. The bundle is self-verifying: the
/// anchor's `state_root` must match the embedded checkpoint state, and every
/// forward block is validated against consensus during `rebuild_state`.
fn checkpoint_chain() -> Option<Blockchain> {
    let path = std::env::var("HIKMALAYER_CHECKPOINT")
        .ok()
        .filter(|v| !v.is_empty())?;
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(err) => {
            eprintln!("⚠️  HIKMALAYER_CHECKPOINT set but {} could not be read: {err}", path);
            return None;
        }
    };
    let bundle = match serde_json::from_slice(&bytes) {
        Ok(bundle) => bundle,
        Err(err) => {
            eprintln!("⚠️  HIKMALAYER_CHECKPOINT {} is not a valid bundle: {err}", path);
            return None;
        }
    };
    match Blockchain::from_bundle(bundle) {
        Ok(chain) => {
            eprintln!(
                "✅ Fast-synced from checkpoint {} — anchored at height {}, tip {}.",
                path,
                chain.base_height,
                chain.tip_index()
            );
            Some(chain)
        }
        Err(err) => {
            eprintln!("⚠️  Checkpoint bundle {} failed verification: {err}", path);
            None
        }
    }
}

fn fresh_chain(difficulty: usize) -> Blockchain {
    // Genesis network parameters. When unset, the well-known DEV defaults
    // apply — suitable for local networks only.
    let supply = std::env::var("GENESIS_SUPPLY")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(DEFAULT_GENESIS_SUPPLY);

    let vrf_key = std::env::var("GENESIS_VALIDATOR_VRF_PUBLIC_KEY")
        .ok()
        .filter(|v| !v.is_empty());

    match (
        std::env::var("GENESIS_TREASURY_ADDRESS").ok().filter(|v| !v.is_empty()),
        std::env::var("GENESIS_VALIDATOR_PUBLIC_KEY").ok().filter(|v| !v.is_empty()),
    ) {
        (Some(treasury), validator_key) => {
            Blockchain::new_with_genesis(difficulty, treasury, validator_key, vrf_key, supply)
        }
        (None, Some(validator_key)) => {
            let treasury = pos::derive_address(&validator_key).unwrap_or_default();
            Blockchain::new_with_genesis(
                difficulty,
                treasury,
                Some(validator_key),
                vrf_key,
                supply,
            )
        }
        (None, None) => {
            eprintln!(
                "⚠️  GENESIS_TREASURY_ADDRESS not set: using the well-known DEV genesis. \
                 Configure real genesis parameters for any shared network."
            );
            Blockchain::new(difficulty)
        }
    }
}

#[tokio::main]
async fn main() {
    let difficulty = 2;

    // Load persisted node state; chain state (balances/stakes/nonces) is
    // deterministically rebuilt from the block history.
    let snapshot = load_state();
    let mut loaded_chain = match snapshot.as_ref().map(|state| state.chain.clone()) {
        // A persisted chain always wins: a running node keeps its own history.
        Some(chain) => chain,
        // No local history yet — fast-sync from a checkpoint bundle if one is
        // configured, otherwise start from a fresh genesis.
        None => checkpoint_chain().unwrap_or_else(|| fresh_chain(difficulty)),
    };
    if let Err(err) = loaded_chain.rebuild_state() {
        eprintln!(
            "⚠️  Persisted chain failed state replay ({}). Starting from a fresh genesis.",
            err
        );
        loaded_chain = fresh_chain(difficulty);
    }
    let chain = Arc::new(Mutex::new(loaded_chain));

    let contracts = Arc::new(Mutex::new(
        snapshot
            .as_ref()
            .map(|state| state.contracts.clone())
            .unwrap_or_else(ContractExecutor::new),
    ));
    let pending_transactions = Arc::new(Mutex::new(
        snapshot
            .as_ref()
            .map(|state| state.pending_transactions.clone())
            .unwrap_or_default(),
    ));
    let auth_manager = Arc::new(Mutex::new(AuthManager::new()));

    // Peers: persisted peers plus any BOOTNODES from the environment.
    let mut initial_peers = snapshot
        .as_ref()
        .map(|state| state.peers.clone())
        .unwrap_or_default();
    if let Ok(bootnodes) = std::env::var("BOOTNODES") {
        for bootnode in bootnodes.split(',') {
            let bootnode = bootnode.trim().to_string();
            if !bootnode.is_empty() && !initial_peers.contains(&bootnode) {
                initial_peers.push(bootnode);
            }
        }
    }
    let peers = Arc::new(Mutex::new(initial_peers));

    let governance = Arc::new(Mutex::new(
        snapshot
            .as_ref()
            .map(|state| state.governance.clone())
            .unwrap_or_default(),
    ));
    let slash_evidence = Arc::new(Mutex::new(
        snapshot
            .as_ref()
            .map(|state| state.slash_evidence.clone())
            .unwrap_or_default(),
    ));
    let metrics = Arc::new(Mutex::new(Metrics::default()));
    let seen_messages = Arc::new(Mutex::new(SeenMessageCache::new(8192)));

    // Optional allow-list: restrict P2P participation to explicit node ids.
    let peer_book = Arc::new(Mutex::new(match std::env::var("P2P_ALLOWLIST") {
        Ok(list) if !list.is_empty() => {
            let allow: std::collections::HashSet<String> = list
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            println!("🔐 P2P allow-list active: {} permitted node(s).", allow.len());
            PeerBook::with_allow_list(allow)
        }
        _ => PeerBook::new(),
    }));

    // Token rotation (HM-06): a node accepts the CURRENT token and, during a
    // rotation window, the PREVIOUS one. The legacy single-token variables
    // remain supported.
    fn load_token_list(legacy_var: &str, current_var: &str, previous_var: &str) -> Vec<String> {
        let mut tokens = Vec::new();
        for var in [current_var, legacy_var, previous_var] {
            if let Ok(value) = std::env::var(var) {
                if !value.is_empty() && !tokens.contains(&value) {
                    tokens.push(value);
                }
            }
        }
        tokens
    }

    let p2p_tokens = load_token_list("P2P_TOKEN", "P2P_TOKEN_CURRENT", "P2P_TOKEN_PREVIOUS");
    let admin_tokens = load_token_list("ADMIN_TOKEN", "ADMIN_TOKEN_CURRENT", "ADMIN_TOKEN_PREVIOUS");

    if p2p_tokens.is_empty() {
        eprintln!("⚠️  No P2P token set (P2P_TOKEN / P2P_TOKEN_CURRENT): all P2P endpoints will reject requests.");
    }
    if admin_tokens.is_empty() {
        eprintln!("⚠️  No admin token set (ADMIN_TOKEN / ADMIN_TOKEN_CURRENT): all admin endpoints will reject requests.");
    }

    // This node's validator identity. The private key never leaves the node.
    let validator_key = match std::env::var("VALIDATOR_PRIVATE_KEY") {
        Ok(private_key) if !private_key.is_empty() => {
            match LocalValidatorKey::from_private_key(&private_key) {
                Ok(key) => {
                    println!("🔑 Local validator identity: {}", key.address);
                    Some(key)
                }
                Err(err) => {
                    eprintln!("❌ Invalid VALIDATOR_PRIVATE_KEY: {}. Continuing without a local validator identity.", err);
                    None
                }
            }
        }
        _ => {
            eprintln!(
                "ℹ️  VALIDATOR_PRIVATE_KEY not set: this node cannot mine directly. \
                 Validators can still use /mine/propose + /mine/submit."
            );
            None
        }
    };

    // Optional treasury key enabling the dev faucet (signed treasury
    // transfers). Never set this on a production node.
    let treasury_key = match std::env::var("TREASURY_PRIVATE_KEY") {
        Ok(private_key) if !private_key.is_empty() => {
            match LocalValidatorKey::from_private_key(&private_key) {
                Ok(key) => {
                    println!("🚰 Faucet enabled for treasury: {}", key.address);
                    Some(key)
                }
                Err(err) => {
                    eprintln!("❌ Invalid TREASURY_PRIVATE_KEY: {}. Faucet disabled.", err);
                    None
                }
            }
        }
        _ => None,
    };

    // Node identity for signed P2P handshakes: dedicated NODE_PRIVATE_KEY,
    // else reuse the validator key, else anonymous (token-only).
    let node_private_key = std::env::var("NODE_PRIVATE_KEY")
        .ok()
        .filter(|k| !k.is_empty())
        .or_else(|| std::env::var("VALIDATOR_PRIVATE_KEY").ok().filter(|k| !k.is_empty()));
    let p2p_require_identity = std::env::var("P2P_REQUIRE_IDENTITY")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);

    let p2p_service = Arc::new(
        P2PService::with_identity(
            std::env::var("NODE_ID").unwrap_or_else(|_| "node-local".to_string()),
            p2p_tokens.first().cloned(),
            node_private_key,
        )
        .unwrap_or_else(|err| panic!("{}", err)),
    );
    if p2p_require_identity {
        println!("🛡️  P2P identity enforcement ON: inbound envelopes must be node-signed.");
    }
    println!("🕸️  P2P node identity: {}", p2p_service.node_id);

    let finality_depth = {
        let governance = governance.lock().await;
        governance.finality_depth
    };
    {
        let mut chain = chain.lock().await;
        chain.apply_finality(finality_depth);
    }

    let app_state = AppState {
        chain,
        contracts,
        pending_transactions,
        auth_manager,
        peers,
        governance,
        slash_evidence,
        metrics,
        seen_messages,
        peer_book,
        p2p_tokens,
        admin_tokens,
        p2p_service,
        p2p_require_identity,
        validator_key,
        treasury_key,
    };

    // Configure CORS to allow React app on localhost:5173
    let cors = CorsLayer::new()
        .allow_origin(
            "http://localhost:5173"
                .parse::<axum::http::HeaderValue>()
                .unwrap(),
        )
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers(Any)
        .allow_credentials(false);

    // Combine API routes with auth routes. Request bodies are capped to
    // bound memory per request.
    let app = api_routes()
        .merge(auth_routes())
        .with_state(app_state)
        .layer(cors)
        .layer(axum::extract::DefaultBodyLimit::max(1_048_576));

    let port = std::env::var("PORT")
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(3000);

    println!("🚀 Hikmalayer REST API running on http://127.0.0.1:{}", port);
    println!("🌐 CORS enabled for React app on http://localhost:5173");
    println!("📋 Available endpoints:");
    println!("  🔐 AUTHENTICATION:");
    println!("      🎫 POST /auth/nonce");
    println!("      ✅ POST /auth/verify (native signature)");
    println!("      🚪 DELETE /auth/logout");
    println!("  🎓 CERTIFICATES:");
    println!("      📜 POST /certificates/issue (admin)");
    println!("      🔍 POST /certificates/verify");
    println!("      ✅ POST /certificates/attest (admin)");
    println!("  💰 TOKENS:");
    println!("      💸 POST /tokens/transfer (signed, executes on-chain)");
    println!("      🚰 POST /tokens/faucet (admin + treasury key)");
    println!("      📊 GET  /tokens/balance/{{account}}");
    println!("      🔢 GET  /tokens/nonce/{{account}}");
    println!("  📦 BLOCKCHAIN:");
    println!("      📚 GET  /blocks");
    println!("      🔢 GET  /blocks/{{index}}");
    println!("      📊 GET  /blockchain/stats");
    println!("      🌳 GET  /blockchain/state (state root & supply)");
    println!("  ⛏️  MINING:");
    println!("      ⚡ POST /mine (local validator key)");
    println!("      📝 POST /mine/propose");
    println!("      📮 POST /mine/submit (signed block)");
    println!("      ⚙️  GET  /mining/difficulty");
    println!("      ⚙️  POST /mining/difficulty (admin)");
    println!("  🧮 STAKING (on-chain):");
    println!("      ➕ POST /staking/deposit (signed stake tx)");
    println!("      ➖ POST /staking/withdraw (signed withdraw tx)");
    println!("      👥 GET  /staking/validators");
    println!("  ⚔️  SLASHING:");
    println!("      🧾 POST /slashing/equivocation (permissionless proof)");
    println!("      🗂  POST /slashing/evidence (admin diagnostics)");
    println!("  ✔️  VALIDATION:");
    println!("      🔍 GET  /blockchain/validate");
    println!("      🔎 GET  /blocks/{{index}}/validate");
    println!("      📋 GET  /validate (tutorial compat)");
    println!("  📄 TRANSACTIONS:");
    println!("      ⏳ GET  /transactions/pending");
    println!();
    println!("🌟 Hybrid PoS+PoW chain with a replicated on-chain state machine!");

    let listener = TcpListener::bind(("0.0.0.0", port)).await.unwrap();

    axum::serve(listener, app).await.unwrap();
}
