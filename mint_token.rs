//! R-05 remediation: operator CLI to mint a fresh signed token for a given
//! scope and TTL. This is the rotation mechanism — no redeploy or restart
//! is needed to rotate; old tokens simply expire on schedule.
//!
//! Usage:
//!   ADMIN_TOKEN_SIGNING_KEY=<hex> cargo run --bin mint_token -- --scope admin --ttl 86400
//!   P2P_TOKEN_SIGNING_KEY=<hex>   cargo run --bin mint_token -- --scope p2p   --ttl 2592000

use clap::Parser;
use hikmalayer_core::auth::token::{generate_token, signing_key_from_env, Scope};

#[derive(Parser)]
#[command(name = "mint_token", about = "Mint a signed, self-expiring admin or P2P token")]
struct Args {
    /// Which credential to mint: "admin" or "p2p"
    #[arg(long, value_parser = ["admin", "p2p"])]
    scope: String,

    /// Time-to-live in seconds
    #[arg(long)]
    ttl: u64,
}

fn main() {
    let args = Args::parse();

    let (env_var, scope) = match args.scope.as_str() {
        "admin" => ("ADMIN_TOKEN_SIGNING_KEY", Scope::Admin),
        "p2p" => ("P2P_TOKEN_SIGNING_KEY", Scope::P2p),
        _ => unreachable!("value_parser restricts this to admin/p2p"),
    };

    let signing_key = match signing_key_from_env(env_var) {
        Some(key) => key,
        None => {
            // Fail-closed: exit with an error, never mint against a missing/malformed key.
            eprintln!(
                "error: {} is not set or is not a valid 64-character hex string (try: openssl rand -hex 32)",
                env_var
            );
            std::process::exit(1);
        }
    };

    let token = generate_token(&signing_key, scope, args.ttl);
    println!("{}", token);
}
