use axum::http::Method;
use hikmalayer::api::routes::{api_routes, AppState, LocalValidatorKey, Metrics};
use hikmalayer::auth::{routes::auth_routes, AuthManager};
use hikmalayer::blockchain::chain::{Blockchain, DEFAULT_GENESIS_SUPPLY};
use hikmalayer::consensus::pos;
use hikmalayer::contract::contract::ContractExecutor;
use hikmalayer::p2p::{protocol::SeenMessageCache, service::P2PService};
use hikmalayer::persistence::load_state;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use tower_http::cors::{Any, CorsLayer};

fn fresh_chain(difficulty: usize) -> Blockchain {
    // Genesis network parameters. When unset, the well-known DEV defaults
    // apply — suitable for local networks only.
    let supply = std::env::var("GENESIS_SUPPLY")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(DEFAULT_GENESIS_SUPPLY);

    match (
        std::env::var("GENESIS_TREASURY_ADDRESS").ok().filter(|v| !v.is_empty()),
        std::env::var("GENESIS_VALIDATOR_PUBLIC_KEY").ok().filter(|v| !v.is_empty()),
    ) {
        (Some(treasury), validator_key) => {
            Blockchain::new_with_genesis(difficulty, treasury, validator_key, supply)
        }
        (None, Some(validator_key)) => {
            let treasury = pos::derive_address(&validator_key).unwrap_or_default();
            Blockchain::new_with_genesis(difficulty, treasury, Some(validator_key), supply)
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
    let mut loaded_chain = snapshot
        .as_ref()
        .map(|state| state.chain.clone())
        .unwrap_or_else(|| fresh_chain(difficulty));
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

    let p2p_token = std::env::var("P2P_TOKEN").ok().filter(|t| !t.is_empty());
    let admin_token = std::env::var("ADMIN_TOKEN").ok().filter(|t| !t.is_empty());

    if p2p_token.is_none() {
        eprintln!("⚠️  P2P_TOKEN is not set: all P2P endpoints will reject requests.");
    }
    if admin_token.is_none() {
        eprintln!("⚠️  ADMIN_TOKEN is not set: all admin endpoints will reject requests.");
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

    let p2p_service = Arc::new(
        P2PService::new(
            std::env::var("NODE_ID").unwrap_or_else(|_| "node-local".to_string()),
            p2p_token.clone(),
        )
        .unwrap_or_else(|err| panic!("{}", err)),
    );

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
        p2p_token,
        admin_token,
        p2p_service,
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

    // Combine API routes with auth routes
    let app = api_routes()
        .merge(auth_routes())
        .with_state(app_state)
        .layer(cors);

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
