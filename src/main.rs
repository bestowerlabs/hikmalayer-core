use axum::http::Method;
use hikmalayer::api::routes::{api_routes, AppState, LocalValidatorKey, Metrics};
use hikmalayer::auth::{routes::auth_routes, AuthManager};
use hikmalayer::blockchain::{chain::Blockchain, transaction::Transaction};
use hikmalayer::consensus::pos::Staker;
use hikmalayer::contract::contract::ContractExecutor;
use hikmalayer::p2p::{protocol::SeenMessageCache, service::P2PService};
use hikmalayer::persistence::load_state;
use hikmalayer::token::fungible::Token;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use tower_http::cors::{Any, CorsLayer};

#[tokio::main]
async fn main() {
    let difficulty = 2;

    // Initialize Blockchain, Token, Contracts, and Pending Transactions
    let snapshot = load_state();
    let chain = Arc::new(Mutex::new(
        snapshot
            .as_ref()
            .map(|state| state.chain.clone())
            .unwrap_or_else(|| Blockchain::new(difficulty)),
    ));
    let token = Arc::new(Mutex::new(
        snapshot
            .as_ref()
            .map(|state| state.token.clone())
            .unwrap_or_else(|| Token::new("Metacation Token", "MCT", 1000, "admin")),
    ));
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
            .unwrap_or_else(Vec::<Transaction>::new),
    ));
    let auth_manager = Arc::new(Mutex::new(AuthManager::new()));
    let stakers = Arc::new(Mutex::new(
        snapshot
            .as_ref()
            .map(|state| state.stakers.clone())
            .unwrap_or_else(Vec::<Staker>::new),
    ));

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
    let nonces = Arc::new(Mutex::new(
        snapshot
            .as_ref()
            .map(|state| state.nonces.clone())
            .unwrap_or_else(HashMap::new),
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
        if !chain.is_valid() {
            eprintln!(
                "⚠️  Loaded chain state failed validation (possibly a pre-upgrade format). \
                 Starting from a fresh genesis. Old state remains on disk until overwritten."
            );
            *chain = Blockchain::new(difficulty);
        }
    }

    let app_state = AppState {
        chain,
        token,
        contracts,
        pending_transactions,
        auth_manager,
        stakers,
        peers,
        governance,
        slash_evidence,
        metrics,
        nonces,
        seen_messages,
        p2p_token,
        admin_token,
        p2p_service,
        validator_key,
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
    println!("      ✅ POST /auth/verify");
    println!("      🚪 DELETE /auth/logout");
    println!("  🎓 CERTIFICATES:");
    println!("      📜 POST /certificates/issue (admin)");
    println!("      🔍 POST /certificates/verify");
    println!("      ✅ POST /certificates/attest (admin)");
    println!("  💰 TOKENS:");
    println!("      💸 POST /tokens/transfer (signed)");
    println!("      🚰 POST /tokens/faucet (admin)");
    println!("      📊 GET  /tokens/balance/{{account}}");
    println!("      🔢 GET  /tokens/nonce/{{account}}");
    println!("  📦 BLOCKCHAIN:");
    println!("      📚 GET  /blocks");
    println!("      🔢 GET  /blocks/{{index}}");
    println!("      📊 GET  /blockchain/stats");
    println!("  ⛏️  MINING:");
    println!("      ⚡ POST /mine (local validator key)");
    println!("      📝 POST /mine/propose");
    println!("      📮 POST /mine/submit (signed block)");
    println!("      ⚙️  GET  /mining/difficulty");
    println!("      ⚙️  POST /mining/difficulty (admin)");
    println!("  🧮 STAKING:");
    println!("      ➕ POST /staking/deposit (signed)");
    println!("      ➖ POST /staking/withdraw (signed)");
    println!("      👥 GET  /staking/validators");
    println!("  ✔️  VALIDATION:");
    println!("      🔍 GET  /blockchain/validate");
    println!("      🔎 GET  /blocks/{{index}}/validate");
    println!("      📋 GET  /validate (tutorial compat)");
    println!("  📄 TRANSACTIONS:");
    println!("      ⏳ GET  /transactions/pending");
    println!();
    println!("🌟 Hybrid PoS+PoW chain with signed transactions and fork choice!");

    let listener = TcpListener::bind(("0.0.0.0", port)).await.unwrap();

    axum::serve(listener, app).await.unwrap();
}
