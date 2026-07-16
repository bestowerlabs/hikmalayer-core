//! Operator CLI for minting/rotating ADMIN_TOKEN / P2P_TOKEN replacements.
//!
//! Usage:
//!   mint_token --scope admin --ttl 86400
//!   mint_token --scope p2p   --ttl 2592000
//!
//! Reads the signing key from ADMIN_TOKEN_SIGNING_KEY / P2P_TOKEN_SIGNING_KEY
//! (64-char hex = 32 bytes) and prints a fresh, signed, time-limited token to
//! stdout. Distribute the printed token to the relevant caller(s); the
//! signing key itself is never transmitted or embedded in the token.
//!
//! Rotation procedure:
//!   1. Run this command to mint a new token with an appropriate TTL.
//!   2. Distribute the new token to admin operators / P2P peers.
//!   3. Old tokens remain valid until their own expiry, then die on their own
//!      — no coordinated cutover required.
//!   4. To revoke immediately (e.g. suspected leak), rotate the signing key
//!      itself; this invalidates all outstanding tokens for that scope at once.

use hikmalayer_r05::auth::token::{generate_token, TokenConfig};

fn print_usage_and_exit() -> ! {
    eprintln!("Usage: mint_token --scope <admin|p2p> --ttl <seconds>");
    std::process::exit(1);
}

fn main() {
    let args: Vec<String> = std::env::args().collect();

    let mut scope: Option<String> = None;
    let mut ttl: Option<u64> = None;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--scope" => {
                scope = args.get(i + 1).cloned();
                i += 2;
            }
            "--ttl" => {
                ttl = args.get(i + 1).and_then(|s| s.parse().ok());
                i += 2;
            }
            _ => print_usage_and_exit(),
        }
    }

    let (scope, ttl) = match (scope.as_deref(), ttl) {
        (Some("admin"), Some(ttl)) => ("admin", ttl),
        (Some("p2p"), Some(ttl)) => ("p2p", ttl),
        _ => print_usage_and_exit(),
    };

    let env_var = match scope {
        "admin" => "ADMIN_TOKEN_SIGNING_KEY",
        "p2p" => "P2P_TOKEN_SIGNING_KEY",
        _ => unreachable!(),
    };

    let Some(signing_key) = hikmalayer_r05::auth::token::signing_key_from_env(env_var) else {
        eprintln!(
            "error: {} is unset, not valid hex, or shorter than 32 bytes.\n\
             Generate one with: openssl rand -hex 32",
            env_var
        );
        std::process::exit(1);
    };

    let cfg = TokenConfig {
        signing_key,
        scope: match scope {
            "admin" => "admin",
            "p2p" => "p2p",
            _ => unreachable!(),
        },
    };

    let token = generate_token(&cfg, ttl);
    println!("{}", token);
}
