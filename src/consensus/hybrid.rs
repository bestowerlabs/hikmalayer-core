//! Hybrid classical + post-quantum account identity.
//!
//! # The property this buys
//!
//! A hybrid account is authorized by **two** signatures over the same
//! message: secp256k1 ECDSA and ML-DSA-65. Both must verify. So an attacker
//! must break *both* schemes to forge one transaction — the account stays
//! safe as long as *either* holds.
//!
//! That is the whole point of hybrid, and it is why it is the right thing to
//! deploy now rather than waiting. Nobody can honestly say when a
//! cryptographically relevant quantum computer arrives; equally, ML-DSA is
//! young enough that a classical break would not be shocking. Requiring both
//! means neither surprise is fatal.
//!
//! # Two account types, one chain
//!
//! ```text
//! hkm…  classical  address = SHA256(secp_pub)[..20]
//!                  authorized by an ECDSA signature
//!
//! hkq…  hybrid     address = SHA256(domain ‖ secp_pub ‖ mldsa_pub)[..20]
//!                  authorized by ECDSA *and* ML-DSA
//! ```
//!
//! The address **commits to both public keys**. This is not cosmetic: if the
//! address were derived from the classical key alone, an attacker who broke
//! secp256k1 could present their own ML-DSA key alongside the victim's
//! classical key, and the "hybrid" account would fall to a single break. By
//! hashing both, substituting either key produces a different address, which
//! is a different account.
//!
//! Existing `hkm…` accounts keep working exactly as before. Hybrid is opt-in
//! per account, and a network can require it for new activity via
//! `ChainState::require_hybrid_signatures` once its users have migrated.
//!
//! # Cost
//!
//! A hybrid transaction carries ~5.3 KB of key and signature material against
//! ~130 bytes classical. That is the price, it is unavoidable with today's
//! post-quantum signatures, and it is why hybrid is opt-in rather than forced
//! on every account from day one.

use sha2::{Digest, Sha256};

use crate::consensus::{pos, pq};

/// Address prefix for hybrid (quantum-ready) accounts.
pub const HYBRID_ADDRESS_PREFIX: &str = "hkq";

/// Domain separator for hybrid address derivation. Keeps a hybrid address
/// from ever colliding with a classical one derived from the same key.
const HYBRID_ADDRESS_DOMAIN: &[u8] = b"hikmalayer-hybrid-address-v1";

/// How an account authorizes itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccountScheme {
    /// secp256k1 ECDSA only. Safe today; not safe against a quantum
    /// adversary that can run Shor's algorithm.
    Classical,
    /// secp256k1 ECDSA **and** ML-DSA-65. Both signatures required.
    Hybrid,
}

/// Which scheme an address commits to, decided by its prefix.
///
/// The address itself carries the answer, so the chain never has to guess
/// what a sender intended, and a hybrid account can never be spent by
/// presenting a classical signature and hoping.
pub fn scheme_of(address: &str) -> Option<AccountScheme> {
    if address.len() != 43 {
        return None;
    }
    let is_hex_lower = |s: &str| s.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase());
    if let Some(rest) = address.strip_prefix(HYBRID_ADDRESS_PREFIX) {
        return is_hex_lower(rest).then_some(AccountScheme::Hybrid);
    }
    if let Some(rest) = address.strip_prefix(pos::ADDRESS_PREFIX) {
        return is_hex_lower(rest).then_some(AccountScheme::Classical);
    }
    None
}

/// Is this a hybrid (quantum-ready) address?
pub fn is_hybrid_address(address: &str) -> bool {
    scheme_of(address) == Some(AccountScheme::Hybrid)
}

/// Derive a hybrid address from both public keys.
///
/// Both keys are bound into the address, so neither can be swapped without
/// changing which account is being named.
pub fn derive_hybrid_address(
    classical_public_key_hex: &str,
    pq_public_key_hex: &str,
) -> Result<String, String> {
    let classical = hex::decode(classical_public_key_hex.trim())
        .map_err(|_| "classical public key is not valid hex".to_string())?;
    // Round-trip through secp256k1 so a malformed point cannot reach the hash
    // and produce an address nobody can ever sign for.
    let parsed = secp256k1::PublicKey::from_slice(&classical)
        .map_err(|_| "classical public key is not a valid secp256k1 point".to_string())?;
    let uncompressed = parsed.serialize_uncompressed();

    if !pq::is_valid_pq_public_key(pq_public_key_hex) {
        return Err("post-quantum public key is not a valid ML-DSA-65 key".to_string());
    }
    let pq_bytes = hex::decode(pq_public_key_hex.trim())
        .map_err(|_| "post-quantum public key is not valid hex".to_string())?;

    let mut hasher = Sha256::new();
    hasher.update(HYBRID_ADDRESS_DOMAIN);
    hasher.update(uncompressed);
    hasher.update(&pq_bytes);
    let hash = hasher.finalize();
    Ok(format!(
        "{}{}",
        HYBRID_ADDRESS_PREFIX,
        hex::encode(&hash[..20])
    ))
}

/// Everything a hybrid account is, derived from the single 32-byte secret a
/// user already backs up. One key to keep, two schemes protecting it.
#[derive(Debug, Clone)]
pub struct HybridIdentity {
    pub address: String,
    pub public_key: String,
    pub pq_public_key: String,
}

/// Derive a hybrid identity from a master secret.
pub fn derive_identity(master_secret_hex: &str) -> Result<HybridIdentity, String> {
    let public_key = pos::derive_public_key(master_secret_hex)?;
    let pq_public_key = pq::derive_pq_public_key(master_secret_hex)?;
    let address = derive_hybrid_address(&public_key, &pq_public_key)?;
    Ok(HybridIdentity {
        address,
        public_key,
        pq_public_key,
    })
}

/// Both signatures over one message.
#[derive(Debug, Clone)]
pub struct HybridSignature {
    pub signature: String,
    pub pq_signature: String,
}

/// Sign a message under both schemes.
pub fn sign_message(message: &str, master_secret_hex: &str) -> Result<HybridSignature, String> {
    Ok(HybridSignature {
        signature: pos::sign_message(message, master_secret_hex)?,
        pq_signature: pq::sign_message(message, master_secret_hex)?,
    })
}

/// Verify that `address` authorized `message`.
///
/// For a hybrid address this checks three things, and all three matter:
///
/// 1. the two public keys together derive to `address` — so neither key can
///    be substituted;
/// 2. the ECDSA signature verifies;
/// 3. the ML-DSA signature verifies.
///
/// Dropping any one of them collapses hybrid back to a single scheme.
pub fn verify_hybrid(
    address: &str,
    message: &str,
    classical_public_key_hex: &str,
    pq_public_key_hex: &str,
    signature_hex: &str,
    pq_signature_hex: &str,
) -> Result<(), String> {
    let derived = derive_hybrid_address(classical_public_key_hex, pq_public_key_hex)?;
    if derived != address {
        return Err("Sender address does not match the hybrid key pair".to_string());
    }
    if !pos::verify_message(message, classical_public_key_hex, signature_hex) {
        return Err("Classical signature verification failed".to_string());
    }
    if !pq::verify_message(message, pq_public_key_hex, pq_signature_hex) {
        return Err("Post-quantum signature verification failed".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY_A: &str = "80f91adc283392febbfc86b7327c055b8559373459040e07e78640e3ac592517";
    const KEY_B: &str = "1111111111111111111111111111111111111111111111111111111111111111";

    #[test]
    fn a_hybrid_identity_is_deterministic_and_well_formed() {
        let first = derive_identity(KEY_A).unwrap();
        let second = derive_identity(KEY_A).unwrap();
        assert_eq!(first.address, second.address);
        assert!(first.address.starts_with(HYBRID_ADDRESS_PREFIX));
        assert_eq!(first.address.len(), 43);
        assert_eq!(scheme_of(&first.address), Some(AccountScheme::Hybrid));
    }

    /// A hybrid address must be distinguishable from the classical address of
    /// the same key — they are different accounts with different rules.
    #[test]
    fn hybrid_and_classical_addresses_of_one_key_differ() {
        let hybrid = derive_identity(KEY_A).unwrap();
        let classical = pos::derive_address(&pos::derive_public_key(KEY_A).unwrap()).unwrap();
        assert_ne!(hybrid.address, classical);
        assert_eq!(scheme_of(&classical), Some(AccountScheme::Classical));
    }

    #[test]
    fn signs_and_verifies_under_both_schemes() {
        let id = derive_identity(KEY_A).unwrap();
        let sig = sign_message("pay alice 10", KEY_A).unwrap();
        assert!(verify_hybrid(
            &id.address,
            "pay alice 10",
            &id.public_key,
            &id.pq_public_key,
            &sig.signature,
            &sig.pq_signature,
        )
        .is_ok());
    }

    /// The point of the whole exercise: a valid ECDSA signature alone must
    /// not move a hybrid account. This is what an attacker holding a broken
    /// secp256k1 would have.
    #[test]
    fn a_classical_signature_alone_cannot_authorize_a_hybrid_account() {
        let id = derive_identity(KEY_A).unwrap();
        let classical_only = pos::sign_message("pay alice 10", KEY_A).unwrap();
        // Genuine ECDSA signature, garbage where ML-DSA should be.
        let result = verify_hybrid(
            &id.address,
            "pay alice 10",
            &id.public_key,
            &id.pq_public_key,
            &classical_only,
            &"00".repeat(pq::PQ_SIGNATURE_LEN),
        );
        assert!(result.is_err(), "hybrid account fell to a classical signature");
    }

    /// And the converse: breaking ML-DSA alone is not enough either.
    #[test]
    fn a_post_quantum_signature_alone_cannot_authorize_a_hybrid_account() {
        let id = derive_identity(KEY_A).unwrap();
        let pq_only = pq::sign_message("pay alice 10", KEY_A).unwrap();
        let result = verify_hybrid(
            &id.address,
            "pay alice 10",
            &id.public_key,
            &id.pq_public_key,
            &"00".repeat(64),
            &pq_only,
        );
        assert!(result.is_err());
    }

    /// An attacker who broke secp256k1 would hold the victim's classical key
    /// and could sign with it — but they do not have the victim's ML-DSA key,
    /// so they would substitute their own. Binding both keys into the address
    /// is what defeats that.
    #[test]
    fn substituting_the_post_quantum_key_changes_the_account() {
        let victim = derive_identity(KEY_A).unwrap();
        let attacker = derive_identity(KEY_B).unwrap();

        // Victim's classical key, attacker's PQ key, both signatures valid
        // for their respective keys.
        let classical = pos::sign_message("drain", KEY_A).unwrap();
        let pq_sig = pq::sign_message("drain", KEY_B).unwrap();

        let result = verify_hybrid(
            &victim.address,
            "drain",
            &victim.public_key,
            &attacker.pq_public_key,
            &classical,
            &pq_sig,
        );
        assert!(
            result.is_err(),
            "a substituted post-quantum key was accepted for the victim's address"
        );
    }

    /// Symmetrically, substituting the classical key must fail too.
    #[test]
    fn substituting_the_classical_key_changes_the_account() {
        let victim = derive_identity(KEY_A).unwrap();
        let attacker = derive_identity(KEY_B).unwrap();
        let classical = pos::sign_message("drain", KEY_B).unwrap();
        let pq_sig = pq::sign_message("drain", KEY_A).unwrap();

        assert!(verify_hybrid(
            &victim.address,
            "drain",
            &attacker.public_key,
            &victim.pq_public_key,
            &classical,
            &pq_sig,
        )
        .is_err());
    }

    #[test]
    fn rejects_a_tampered_message() {
        let id = derive_identity(KEY_A).unwrap();
        let sig = sign_message("pay alice 10", KEY_A).unwrap();
        assert!(verify_hybrid(
            &id.address,
            "pay alice 1000",
            &id.public_key,
            &id.pq_public_key,
            &sig.signature,
            &sig.pq_signature,
        )
        .is_err());
    }

    #[test]
    fn address_classification_rejects_malformed_input() {
        assert_eq!(scheme_of(""), None);
        assert_eq!(scheme_of("hkq"), None);
        assert_eq!(scheme_of(&format!("hkq{}", "z".repeat(40))), None);
        // Uppercase hex would give one key two spellings.
        let id = derive_identity(KEY_A).unwrap();
        assert_eq!(scheme_of(&id.address.to_uppercase()), None);
        assert_eq!(scheme_of(&id.address[..42]), None);
    }

    #[test]
    fn rejects_malformed_keys_when_deriving() {
        let id = derive_identity(KEY_A).unwrap();
        assert!(derive_hybrid_address("not-hex", &id.pq_public_key).is_err());
        assert!(derive_hybrid_address(&id.public_key, "not-hex").is_err());
        // A syntactically fine but invalid curve point must not become an
        // address nobody could ever sign for.
        assert!(derive_hybrid_address(&"04".repeat(65), &id.pq_public_key).is_err());
    }
}
