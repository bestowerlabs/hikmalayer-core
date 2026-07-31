//! Offline wallet and validator signing tool.
//!
//! Keys generated here never touch a node: block proposals and account
//! operations are signed locally and only the signatures are submitted.
//!
//! Usage:
//!   hikma-wallet keygen
//!   hikma-wallet address <public_key_hex>
//!   hikma-wallet sign-block <block_hash_hex> <private_key_hex>
//!   hikma-wallet vrf-prove <slot_input> <private_key_hex>
//!   hikma-wallet sign-transfer <from> <to> <amount> <nonce> <private_key_hex>
//!   hikma-wallet sign-stake <address> <amount> <nonce> <private_key_hex>
//!   hikma-wallet sign-withdraw <address> <amount> <nonce> <private_key_hex>
//!   hikma-wallet sign-vest <from> <to> <amount> <cliff_blocks> <duration_blocks> <nonce> <private_key_hex>
//!   hikma-wallet sign-token-create <symbol> <name> <decimals> <initial_supply> <nonce> <private_key_hex>
//!   hikma-wallet sign-token-transfer <token_id> <to> <amount> <nonce> <private_key_hex>
//!   hikma-wallet sign-token-burn <token_id> <amount> <nonce> <private_key_hex>
//!   hikma-wallet sign-amm-add <token_id> <amount_hkm> <amount_token> <min_shares> <nonce> <private_key_hex>
//!   hikma-wallet sign-amm-remove <token_id> <shares> <min_hkm> <min_token> <nonce> <private_key_hex>
//!   hikma-wallet sign-amm-swap <token_id> <hkm_to_token> <amount_in> <min_out> <nonce> <private_key_hex>
//!   hikma-wallet sign-credential <id> <subject> <data_hash> <revoke> <nonce> <private_key_hex>

use hikmalayer::blockchain::transaction::{CredentialAction, Transaction};
use hikmalayer::blockchain::state::DEFAULT_CHAIN_ID;
use hikmalayer::consensus::{pos, vrf};
use rand::RngCore;
use secp256k1::SecretKey;

fn generate_private_key() -> String {
    let mut rng = rand::rng();
    loop {
        let mut bytes = [0u8; 32];
        rng.fill_bytes(&mut bytes);
        if SecretKey::from_slice(&bytes).is_ok() {
            return hex::encode(bytes);
        }
    }
}

fn keygen() -> Result<(), String> {
    let private_key = generate_private_key();
    let public_key = pos::derive_public_key(&private_key)?;
    let address = pos::derive_address(&public_key)?;
    let vrf_public_key = vrf::derive_vrf_public_key(&private_key)?;
    println!("private_key:    {}", private_key);
    println!("public_key:     {}", public_key);
    println!("vrf_public_key: {}", vrf_public_key);
    println!("address:        {}", address);
    println!();
    println!("Keep the private key offline. Only the public keys and address are shared.");
    Ok(())
}

/// Sign a canonical message, scoped to a network.
///
/// The network comes from `HIKMALAYER_CHAIN_ID` (default the dev network).
/// It is part of what gets signed, so a signature produced for one network
/// is inert on every other — set it to match the chain you are signing for,
/// or the node will reject the signature.
fn sign_and_print(message: &str, private_key: &str) -> Result<(), String> {
    let chain_id = std::env::var("HIKMALAYER_CHAIN_ID")
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| DEFAULT_CHAIN_ID.to_string());
    let message = &Transaction::scoped_signing_message(&chain_id, message);
    let public_key = pos::derive_public_key(private_key)?;
    let signature = pos::sign_message(message, private_key)?;
    println!("chain_id:   {}", chain_id);
    println!("message:    {}", message);
    println!("public_key: {}", public_key);
    println!("signature:  {}", signature);
    Ok(())
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let usage = "Commands:\n  keygen\n  address <public_key_hex>\n  sign-block <block_hash_hex> <private_key_hex>\n  vrf-prove <slot_input> <private_key_hex>\n  sign-transfer <from> <to> <amount> <nonce> <private_key_hex>\n  sign-stake <address> <amount> <nonce> <private_key_hex>\n  sign-withdraw <address> <amount> <nonce> <private_key_hex>\n  sign-vest <from> <to> <amount> <cliff_blocks> <duration_blocks> <nonce> <private_key_hex>\n  sign-token-create <symbol> <name> <decimals> <initial_supply> <nonce> <private_key_hex>\n  sign-token-transfer <token_id> <to> <amount> <nonce> <private_key_hex>\n  sign-token-burn <token_id> <amount> <nonce> <private_key_hex>\n  sign-amm-add <token_id> <amount_hkm> <amount_token> <min_shares> <nonce> <private_key_hex>\n  sign-amm-remove <token_id> <shares> <min_hkm> <min_token> <nonce> <private_key_hex>\n  sign-amm-swap <token_id> <hkm_to_token> <amount_in> <min_out> <nonce> <private_key_hex>\n  sign-credential <id> <subject> <data_hash> <revoke> <nonce> <private_key_hex>";

    match args.first().map(String::as_str) {
        Some("keygen") => keygen(),
        Some("address") => {
            let public_key = args.get(1).ok_or(usage)?;
            println!("address: {}", pos::derive_address(public_key)?);
            Ok(())
        }
        Some("sign-block") => {
            let block_hash = args.get(1).ok_or(usage)?;
            let private_key = args.get(2).ok_or(usage)?;
            let signature = pos::sign_block_hash(block_hash, private_key)?;
            println!("block_hash: {}", block_hash);
            println!("signature:  {}", signature);
            Ok(())
        }
        Some("sign-transfer") => {
            let (from, to) = (args.get(1).ok_or(usage)?, args.get(2).ok_or(usage)?);
            let amount: u64 = args
                .get(3)
                .ok_or(usage)?
                .parse()
                .map_err(|_| "amount must be a number".to_string())?;
            let nonce: u64 = args
                .get(4)
                .ok_or(usage)?
                .parse()
                .map_err(|_| "nonce must be a number".to_string())?;
            let private_key = args.get(5).ok_or(usage)?;
            let message = Transaction::transfer_signing_message(from, to, amount, nonce);
            sign_and_print(&message, private_key)
        }
        Some("sign-vest") => {
            let (from, to) = (args.get(1).ok_or(usage)?, args.get(2).ok_or(usage)?);
            let amount: u64 = args
                .get(3)
                .ok_or(usage)?
                .parse()
                .map_err(|_| "amount must be a number".to_string())?;
            let cliff: u64 = args
                .get(4)
                .ok_or(usage)?
                .parse()
                .map_err(|_| "cliff_blocks must be a number".to_string())?;
            let duration: u64 = args
                .get(5)
                .ok_or(usage)?
                .parse()
                .map_err(|_| "duration_blocks must be a number".to_string())?;
            let nonce: u64 = args
                .get(6)
                .ok_or(usage)?
                .parse()
                .map_err(|_| "nonce must be a number".to_string())?;
            let private_key = args.get(7).ok_or(usage)?;
            let message =
                Transaction::vest_signing_message(from, to, amount, cliff, duration, nonce);
            sign_and_print(&message, private_key)
        }
        Some("sign-token-create") => {
            let symbol = args.get(1).ok_or(usage)?;
            let name = args.get(2).ok_or(usage)?;
            let decimals: u32 = args
                .get(3)
                .ok_or(usage)?
                .parse()
                .map_err(|_| "decimals must be a number".to_string())?;
            let initial_supply: u64 = args
                .get(4)
                .ok_or(usage)?
                .parse()
                .map_err(|_| "initial_supply must be a number".to_string())?;
            let nonce: u64 = args
                .get(5)
                .ok_or(usage)?
                .parse()
                .map_err(|_| "nonce must be a number".to_string())?;
            let private_key = args.get(6).ok_or(usage)?;
            let message = Transaction::token_create_signing_message(
                symbol,
                name,
                decimals,
                initial_supply,
                nonce,
            );
            sign_and_print(&message, private_key)
        }
        Some("sign-token-transfer") => {
            let (token_id, to) = (args.get(1).ok_or(usage)?, args.get(2).ok_or(usage)?);
            let amount: u64 = args
                .get(3)
                .ok_or(usage)?
                .parse()
                .map_err(|_| "amount must be a number".to_string())?;
            let nonce: u64 = args
                .get(4)
                .ok_or(usage)?
                .parse()
                .map_err(|_| "nonce must be a number".to_string())?;
            let private_key = args.get(5).ok_or(usage)?;
            let message =
                Transaction::token_transfer_signing_message(token_id, to, amount, nonce);
            sign_and_print(&message, private_key)
        }
        Some("sign-token-burn") => {
            let token_id = args.get(1).ok_or(usage)?;
            let amount: u64 = args
                .get(2)
                .ok_or(usage)?
                .parse()
                .map_err(|_| "amount must be a number".to_string())?;
            let nonce: u64 = args
                .get(3)
                .ok_or(usage)?
                .parse()
                .map_err(|_| "nonce must be a number".to_string())?;
            let private_key = args.get(4).ok_or(usage)?;
            let message = Transaction::token_burn_signing_message(token_id, amount, nonce);
            sign_and_print(&message, private_key)
        }
        Some("sign-amm-add") => {
            let token_id = args.get(1).ok_or(usage)?;
            let amount_hkm: u64 = args
                .get(2)
                .ok_or(usage)?
                .parse()
                .map_err(|_| "amount_hkm must be a number".to_string())?;
            let amount_token: u64 = args
                .get(3)
                .ok_or(usage)?
                .parse()
                .map_err(|_| "amount_token must be a number".to_string())?;
            let min_shares: u64 = args
                .get(4)
                .ok_or(usage)?
                .parse()
                .map_err(|_| "min_shares must be a number".to_string())?;
            let nonce: u64 = args
                .get(5)
                .ok_or(usage)?
                .parse()
                .map_err(|_| "nonce must be a number".to_string())?;
            let private_key = args.get(6).ok_or(usage)?;
            let message = Transaction::amm_add_signing_message(
                token_id,
                amount_hkm,
                amount_token,
                min_shares,
                nonce,
            );
            sign_and_print(&message, private_key)
        }
        Some("sign-amm-remove") => {
            let token_id = args.get(1).ok_or(usage)?;
            let shares: u64 = args
                .get(2)
                .ok_or(usage)?
                .parse()
                .map_err(|_| "shares must be a number".to_string())?;
            let min_hkm: u64 = args
                .get(3)
                .ok_or(usage)?
                .parse()
                .map_err(|_| "min_hkm must be a number".to_string())?;
            let min_token: u64 = args
                .get(4)
                .ok_or(usage)?
                .parse()
                .map_err(|_| "min_token must be a number".to_string())?;
            let nonce: u64 = args
                .get(5)
                .ok_or(usage)?
                .parse()
                .map_err(|_| "nonce must be a number".to_string())?;
            let private_key = args.get(6).ok_or(usage)?;
            let message = Transaction::amm_remove_signing_message(
                token_id, shares, min_hkm, min_token, nonce,
            );
            sign_and_print(&message, private_key)
        }
        Some("sign-amm-swap") => {
            let token_id = args.get(1).ok_or(usage)?;
            let hkm_to_token: bool = args
                .get(2)
                .ok_or(usage)?
                .parse()
                .map_err(|_| "hkm_to_token must be true or false".to_string())?;
            let amount_in: u64 = args
                .get(3)
                .ok_or(usage)?
                .parse()
                .map_err(|_| "amount_in must be a number".to_string())?;
            let min_out: u64 = args
                .get(4)
                .ok_or(usage)?
                .parse()
                .map_err(|_| "min_out must be a number".to_string())?;
            let nonce: u64 = args
                .get(5)
                .ok_or(usage)?
                .parse()
                .map_err(|_| "nonce must be a number".to_string())?;
            let private_key = args.get(6).ok_or(usage)?;
            let message = Transaction::amm_swap_signing_message(
                token_id,
                hkm_to_token,
                amount_in,
                min_out,
                nonce,
            );
            sign_and_print(&message, private_key)
        }
        Some("vrf-prove") => {
            let slot_input = args.get(1).ok_or(usage)?;
            let private_key = args.get(2).ok_or(usage)?;
            let (output, proof) = vrf::prove(slot_input, private_key)?;
            println!("slot_input:  {}", slot_input);
            println!("vrf_output:  {}", output);
            println!("vrf_proof:   {}", proof);
            Ok(())
        }
        Some("sign-stake") => {
            let address = args.get(1).ok_or(usage)?;
            let amount: u64 = args
                .get(2)
                .ok_or(usage)?
                .parse()
                .map_err(|_| "amount must be a number".to_string())?;
            let nonce: u64 = args
                .get(3)
                .ok_or(usage)?
                .parse()
                .map_err(|_| "nonce must be a number".to_string())?;
            let private_key = args.get(4).ok_or(usage)?;
            // The VRF key is derived from the same secret and bound into
            // the signed stake message.
            let vrf_public_key = vrf::derive_vrf_public_key(private_key)?;
            let message =
                Transaction::stake_signing_message(address, amount, nonce, &vrf_public_key);
            println!("vrf_public_key: {}", vrf_public_key);
            sign_and_print(&message, private_key)
        }
        Some("sign-credential") => {
            let id = args.get(1).ok_or(usage)?;
            let subject = args.get(2).ok_or(usage)?;
            let data_hash = args.get(3).ok_or(usage)?;
            let revoke: bool = args
                .get(4)
                .ok_or(usage)?
                .parse()
                .map_err(|_| "revoke must be true or false".to_string())?;
            let nonce: u64 = args
                .get(5)
                .ok_or(usage)?
                .parse()
                .map_err(|_| "nonce must be a number".to_string())?;
            let private_key = args.get(6).ok_or(usage)?;
            let action = CredentialAction {
                id: id.clone(),
                subject: subject.clone(),
                data_hash: data_hash.clone(),
                revoke,
            };
            let message = Transaction::credential_signing_message(&action, nonce);
            sign_and_print(&message, private_key)
        }
        Some("sign-withdraw") => {
            let address = args.get(1).ok_or(usage)?;
            let amount: u64 = args
                .get(2)
                .ok_or(usage)?
                .parse()
                .map_err(|_| "amount must be a number".to_string())?;
            let nonce: u64 = args
                .get(3)
                .ok_or(usage)?
                .parse()
                .map_err(|_| "nonce must be a number".to_string())?;
            let private_key = args.get(4).ok_or(usage)?;
            let message = Transaction::withdraw_signing_message(address, amount, nonce);
            sign_and_print(&message, private_key)
        }
        _ => Err(usage.to_string()),
    }
}

fn main() {
    if let Err(err) = run() {
        eprintln!("{}", err);
        std::process::exit(1);
    }
}
