use rand::Rng;
use secp256k1::{ecdsa::Signature, Message, PublicKey, Secp256k1, SecretKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Native Hikmalayer address prefix. Addresses are derived with the chain's
/// own hash (SHA-256) — no external chain conventions.
pub const ADDRESS_PREFIX: &str = "hkm";

/// Domain-separation prefix for signed messages, so a Hikmalayer signature
/// can never be replayed as (or confused with) another system's signature.
pub const MESSAGE_PREFIX: &str = "\x19Hikmalayer Signed Message:\n";

/// A registered validator. The server never stores or accepts private keys
/// (HM-01): validators sign block proposals and account operations locally.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Staker {
    pub address: String,
    pub stake: u64,
    pub public_key: Option<String>,
}

const SLASH_PERCENT: u64 = 10;

/// Deterministic seed for validator selection at a given height. Salting the
/// parent hash with the height means the same parent hash can never be reused
/// to claim a different slot.
pub fn selection_seed(previous_hash: &str, block_index: u64) -> String {
    format!("{}:{}", previous_hash, block_index)
}

fn seed_to_u64(seed: &str) -> u64 {
    let digest = Sha256::digest(seed.as_bytes());
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&digest[..8]);
    u64::from_be_bytes(bytes)
}

pub fn select_staker_with_seed(seed: &str, stakers: &[Staker]) -> Option<String> {
    let total_stake: u64 = stakers.iter().map(|s| s.stake).sum();

    if total_stake == 0 {
        return None;
    }

    let selection_point = seed_to_u64(seed) % total_stake;
    let mut running_point = selection_point;

    for staker in stakers {
        if running_point < staker.stake {
            return Some(staker.address.clone());
        }
        running_point -= staker.stake;
    }

    None
}

pub fn select_staker(stakers: &[Staker]) -> Option<String> {
    let total_stake: u64 = stakers.iter().map(|s| s.stake).sum();

    if total_stake == 0 {
        return None;
    }

    let mut rng = rand::rng();
    let selection_point = rng.random_range(0..total_stake);
    let seed = format!("random-{}", selection_point);

    select_staker_with_seed(&seed, stakers)
}

/// Derive the canonical native account address from a secp256k1 public key:
/// `hkm` + hex of the first 20 bytes of SHA-256 over the uncompressed key.
pub fn derive_address(public_key_hex: &str) -> Result<String, String> {
    let public_key_bytes = hex::decode(public_key_hex).map_err(|err| err.to_string())?;
    let public_key = PublicKey::from_slice(&public_key_bytes).map_err(|err| err.to_string())?;
    let uncompressed = public_key.serialize_uncompressed();
    let hash = Sha256::digest(uncompressed);
    Ok(format!("{}{}", ADDRESS_PREFIX, hex::encode(&hash[..20])))
}

/// Derive the uncompressed public key (hex) for a private key.
pub fn derive_public_key(private_key_hex: &str) -> Result<String, String> {
    let secret_key_bytes = hex::decode(private_key_hex).map_err(|err| err.to_string())?;
    let secret_key = SecretKey::from_slice(&secret_key_bytes).map_err(|err| err.to_string())?;
    let secp = Secp256k1::new();
    let public_key = PublicKey::from_secret_key(&secp, &secret_key);
    Ok(hex::encode(public_key.serialize_uncompressed()))
}

fn sign_digest(digest: &[u8], private_key_hex: &str) -> Result<String, String> {
    let message = Message::from_digest_slice(digest).map_err(|err| err.to_string())?;
    let secret_key_bytes = hex::decode(private_key_hex).map_err(|err| err.to_string())?;
    let secret_key = SecretKey::from_slice(&secret_key_bytes).map_err(|err| err.to_string())?;
    let secp = Secp256k1::new();
    let signature = secp.sign_ecdsa(&message, &secret_key);
    Ok(hex::encode(signature.serialize_compact()))
}

fn verify_digest(digest: &[u8], public_key_hex: &str, signature_hex: &str) -> bool {
    let message = match Message::from_digest_slice(digest) {
        Ok(msg) => msg,
        Err(_) => return false,
    };
    let signature_bytes = match hex::decode(signature_hex) {
        Ok(bytes) => bytes,
        Err(_) => return false,
    };
    let signature = match Signature::from_compact(&signature_bytes) {
        Ok(sig) => sig,
        Err(_) => return false,
    };
    let public_key_bytes = match hex::decode(public_key_hex) {
        Ok(bytes) => bytes,
        Err(_) => return false,
    };
    let public_key = match PublicKey::from_slice(&public_key_bytes) {
        Ok(key) => key,
        Err(_) => return false,
    };
    let secp = Secp256k1::new();
    secp.verify_ecdsa(&message, &signature, &public_key).is_ok()
}

/// Sign a 32-byte block hash (hex) with a validator private key. Used by
/// validator tooling only — the node itself never handles foreign keys.
pub fn sign_block_hash(block_hash: &str, private_key_hex: &str) -> Result<String, String> {
    let hash_bytes = hex::decode(block_hash).map_err(|err| err.to_string())?;
    sign_digest(&hash_bytes, private_key_hex)
}

pub fn verify_block_signature(block_hash: &str, public_key_hex: &str, signature_hex: &str) -> bool {
    let hash_bytes = match hex::decode(block_hash) {
        Ok(bytes) => bytes,
        Err(_) => return false,
    };
    verify_digest(&hash_bytes, public_key_hex, signature_hex)
}

/// Digest of a message under the native Hikmalayer signing domain.
fn message_digest(message: &str) -> [u8; 32] {
    let prefixed = format!("{}{}{}", MESSAGE_PREFIX, message.len(), message);
    Sha256::digest(prefixed.as_bytes()).into()
}

/// Sign an arbitrary UTF-8 message under the native signing domain — used
/// for transfer, stake, and withdraw authorizations.
pub fn sign_message(message: &str, private_key_hex: &str) -> Result<String, String> {
    sign_digest(&message_digest(message), private_key_hex)
}

pub fn verify_message(message: &str, public_key_hex: &str, signature_hex: &str) -> bool {
    verify_digest(&message_digest(message), public_key_hex, signature_hex)
}

pub fn slash_staker(stakers: &mut Vec<Staker>, address: &str) -> u64 {
    slash_staker_with_percent(stakers, address, SLASH_PERCENT)
}

pub fn slash_staker_with_percent(stakers: &mut Vec<Staker>, address: &str, percent: u64) -> u64 {
    for staker in stakers.iter_mut() {
        if staker.address == address {
            let slashed = staker.stake.saturating_mul(percent) / 100;
            staker.stake = staker.stake.saturating_sub(slashed);
            return slashed;
        }
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_keys() -> (String, String) {
        let private_key = hex::encode([7u8; 32]);
        let public_key = derive_public_key(&private_key).unwrap();
        (public_key, private_key)
    }

    #[test]
    fn test_select_staker() {
        let stakers = vec![
            Staker {
                address: "Alice".to_string(),
                stake: 100,
                public_key: None,
            },
            Staker {
                address: "Bob".to_string(),
                stake: 50,
                public_key: None,
            },
        ];

        let winner = select_staker_with_seed("seed", &stakers);
        assert!(winner.is_some());
    }

    #[test]
    fn selection_is_deterministic_and_height_salted() {
        let stakers = vec![
            Staker {
                address: "Alice".to_string(),
                stake: 100,
                public_key: None,
            },
            Staker {
                address: "Bob".to_string(),
                stake: 100,
                public_key: None,
            },
        ];

        let seed_a = selection_seed("parent-hash", 5);
        let seed_b = selection_seed("parent-hash", 6);
        assert_ne!(seed_a, seed_b);
        assert_eq!(
            select_staker_with_seed(&seed_a, &stakers),
            select_staker_with_seed(&seed_a, &stakers)
        );
    }

    #[test]
    fn message_signatures_roundtrip() {
        let (public_key, private_key) = test_keys();
        let message = "hikmalayer-transfer:0xabc:0xdef:10:1";
        let signature = sign_message(message, &private_key).unwrap();
        assert!(verify_message(message, &public_key, &signature));
        assert!(!verify_message("tampered", &public_key, &signature));
    }

    #[test]
    fn derive_address_is_native_and_stable() {
        let (public_key, _) = test_keys();
        let address = derive_address(&public_key).unwrap();
        assert!(address.starts_with(ADDRESS_PREFIX));
        assert_eq!(address.len(), ADDRESS_PREFIX.len() + 40);
        assert_eq!(address, derive_address(&public_key).unwrap());
    }

    #[test]
    fn slash_reduces_stake() {
        let mut stakers = vec![Staker {
            address: "Alice".to_string(),
            stake: 100,
            public_key: None,
        }];
        let slashed = slash_staker_with_percent(&mut stakers, "Alice", 25);
        assert_eq!(slashed, 25);
        assert_eq!(stakers[0].stake, 75);
    }
}
