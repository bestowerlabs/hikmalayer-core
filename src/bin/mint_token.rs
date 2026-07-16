//! R-05: operator CLI to mint a fresh signed admin/P2P token for a given
//! scope and TTL. This is the rotation mechanism — no redeploy or restart
//! is needed to rotate; old tokens simply expire on schedule.
//!
//! Usage:
//!   ADMIN_TOKEN_SIGNING_KEY=<hex64> mint_token --scope admin --ttl 86400
//!   P2P_TOKEN_SIGNING_KEY=<hex64>   mint_token --scope p2p   --ttl 2592000
//!
//! Generate a signing key with: openssl rand -hex 32

use hikmalayer::auth::token::{generate_token, signing_key_from_env, Scope};

fn usage() -> ! {
    eprintln!(
        "Usage: mint_token --scope <admin|p2p> [--ttl <seconds>]\n\
         Requires ADMIN_TOKEN_SIGNING_KEY or P2P_TOKEN_SIGNING_KEY (64 hex chars)\n\
         in the environment for the chosen scope. Default TTL: 86400 (1 day)."
    );
    std::process::exit(2);
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut scope: Option<Scope> = None;
    let mut ttl: u64 = 86_400;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--scope" => {
                let value = args.get(i + 1).unwrap_or_else(|| usage());
                scope = match value.as_str() {
                    "admin" => Some(Scope::Admin),
                    "p2p" => Some(Scope::P2p),
                    _ => usage(),
                };
                i += 2;
            }
            "--ttl" => {
                let value = args.get(i + 1).unwrap_or_else(|| usage());
                ttl = value.parse().unwrap_or_else(|_| usage());
                i += 2;
            }
            _ => usage(),
        }
    }

    let scope = scope.unwrap_or_else(|| usage());
    let (env_var, label) = match scope {
        Scope::Admin => ("ADMIN_TOKEN_SIGNING_KEY", "admin"),
        Scope::P2p => ("P2P_TOKEN_SIGNING_KEY", "p2p"),
    };

    let Some(key) = signing_key_from_env(env_var) else {
        eprintln!(
            "ERROR: {env_var} is unset or malformed (need 64 hex chars, e.g. `openssl rand -hex 32`)."
        );
        std::process::exit(1);
    };

    let token = generate_token(&key, scope, ttl);
    eprintln!("Minted {label} token (expires in {ttl}s):");
    println!("{token}");
}
