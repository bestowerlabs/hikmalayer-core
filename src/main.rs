mod api;
mod auth;
mod blockchain;
mod consensus;
mod contract;
mod governance;
mod p2p;
mod persistence;
mod token;

use api::routes::{api_routes, AppState};
use auth::{routes::auth_routes, AuthManager};
use axum::http::Method;
use blockchain::{chain::Blockchain, transaction::Transaction};
use consensus::pos::Staker;
use contract::contract::ContractExecutor;
use p2p::service::P2PService;
use persistence::load_state;
use std::sync::Arc;
use token::fungible::Token;
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
            .unwrap_or_else(|| Vec::<Transaction>::new()),
    ));
    let auth_manager = Arc::new(Mutex::new(AuthManager::new()));
    let stakers = Arc::new(Mutex::new(
        snapshot
            .as_ref()
            .map(|state| state.stakers.clone())
            .unwrap_or_else(|| Vec::<Staker>::new()),
    ));
    let peers = Arc::new(Mutex::new(
        snapshot
            .as_ref()
            .map(|state| state.peers.clone())
            .unwrap_or_else(Vec::new),
    ));
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
    let metrics = Arc::new(Mutex::new(api::routes::Metrics::default()));
    fn load_token_list(current_var: &str, previous_var: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    if let Ok(current) = std::env::var(current_var) {
        if !current.is_empty() {
            tokens.push(current);
        }
    }
    if let Ok(previous) = std::env::var(previous_var) {
        if !previous.is_empty() {
            tokens.push(previous);
        }
    }
    tokens
}

let p2p_tokens = load_token_list("P2P_TOKEN_CURRENT", "P2P_TOKEN_PREVIOUS");
let admin_tokens = load_token_list("ADMIN_TOKEN_CURRENT", "ADMIN_TOKEN_PREVIOUS");
    

    let p2p_service = Arc::new(
        P2PService::new(
            std::env::var("NODE_ID").unwrap_or_else(|_| "node-local".to_string()),
            p2p_tokens.first().cloned(),
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
        token,
        contracts,
        pending_transactions,
        auth_manager,
        stakers,
        peers,
        governance,
        slash_evidence,
        metrics,
        p2p_tokens,
        admin_tokens,
        p2p_service,
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

    println!("🚀 Hikmalayer REST API running on http://127.0.0.1:3000");
    println!("🌐 CORS enabled for React app on http://localhost:5173");
    println!("📋 Available endpoints:");
    println!("  🔐 AUTHENTICATION:");
    println!("      🎫 POST /auth/nonce");
    println!("      ✅ POST /auth/verify");
    println!("      🚪 DELETE /auth/logout");
    println!("  🎓 CERTIFICATES:");
    println!("      📜 POST /certificates/issue");
    println!("      ✅ POST /certificates/verify");
    println!("  💰 TOKENS:");
    println!("      💸 POST /tokens/transfer");
    println!("      📊 GET  /tokens/balance/{{account}}");
    println!("  📦 BLOCKCHAIN:");
    println!("      📚 GET  /blocks");
    println!("      🔢 GET  /blocks/{{index}}");
    println!("      📊 GET  /blockchain/stats");
    println!("  ⛏️  MINING:");
    println!("      ⚡ POST /mine");
    println!("      ⚙️  GET  /mining/difficulty");
    println!("      ⚙️  POST /mining/difficulty");
    println!("  🧮 STAKING:");
    println!("      ➕ POST /staking/deposit");
    println!("      ➖ POST /staking/withdraw");
    println!("      👥 GET  /staking/validators");
    println!("  ✔️  VALIDATION:");
    println!("      🔍 GET  /blockchain/validate");
    println!("      🔎 GET  /blocks/{{index}}/validate");
    println!("      📋 GET  /validate (tutorial compat)");
    println!("  📄 TRANSACTIONS:");
    println!("      ⏳ GET  /transactions/pending");
    println!("");
    println!("🌟 Complete blockchain with wallet authentication & smart contracts!");

    let listener = TcpListener::bind("0.0.0.0:3000").await.unwrap();

    axum::serve(listener, app).await.unwrap();
}
