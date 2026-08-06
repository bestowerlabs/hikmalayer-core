//! Post-quantum signatures: ML-DSA-65 (FIPS 204).
//!
//! # Why this exists
//!
//! secp256k1 — the scheme every account signature and every block signature
//! uses today — is broken by Shor's algorithm. A sufficiently large quantum
//! computer recovers a private key from a public key, and Hikmalayer publishes
//! public keys: every transaction carries one, and every validator's key sits
//! permanently in `StakeInfo`. Hashing (SHA-256) and symmetric crypto are
//! fine — Grover only halves them — but signatures are not.
//!
//! This module adds the second half of a **hybrid** scheme. It does not
//! replace secp256k1; it stands alongside it, so an attacker must break
//! *both* to forge. See [`crate::consensus::hybrid`].
//!
//! # Which parameter set, and why
//!
//! ML-DSA-65 (NIST security category 3). ML-DSA-44 is category 2, which is a
//! thinner margin than a chain holding value for decades should accept;
//! ML-DSA-87 is category 5 and costs another ~1.3 KB per signature for
//! protection against an adversary nobody can presently describe. 65 is the
//! balance, and it is what most deploying systems have chosen.
//!
//! Sizes are the honest cost of this: a 3,309-byte signature and a
//! 1,952-byte public key, against 64 and 65 bytes for secp256k1.
//!
//! # Determinism
//!
//! Keys and signatures are both derived deterministically:
//!
//! * **Keys** from a 32-byte seed ξ, which is FIPS 204's own keygen input
//!   (Algorithm 1), so any conforming implementation given the same ξ
//!   produces the same keypair. That is what lets the browser wallet and the
//!   Rust node agree.
//! * **Signatures** from a seed rather than fresh randomness, so signing the
//!   same message twice gives the same bytes. This is the hedged variant with
//!   the randomness pinned; it is standards-conforming, it makes signatures
//!   reproducible across implementations, and it means a broken RNG on a
//!   user's machine cannot leak a key.

use fips204::ml_dsa_65;
use fips204::traits::{KeyGen, SerDes, Signer, Verifier};
use rand_core::{CryptoRng, RngCore};
use sha2::{Digest, Sha256};

/// Public key length in bytes (FIPS 204, ML-DSA-65).
pub const PQ_PUBLIC_KEY_LEN: usize = ml_dsa_65::PK_LEN;

/// Signature length in bytes.
pub const PQ_SIGNATURE_LEN: usize = ml_dsa_65::SIG_LEN;

/// Domain separator for deriving the ML-DSA seed from an account's master
/// secret. Without it the same 32 bytes would key two different schemes,
/// which is exactly the kind of reuse that turns one break into two.
const PQ_SEED_DOMAIN: &[u8] = b"hikmalayer-pq-key-v1";

/// Domain separator for the per-signature seed.
const PQ_SIGN_DOMAIN: &[u8] = b"hikmalayer-pq-sign-v1";

/// FIPS 204 takes an application context string; ours names the chain so an
/// ML-DSA signature made for Hikmalayer cannot be lifted into another
/// protocol that also uses ML-DSA.
const PQ_CONTEXT: &[u8] = b"hikmalayer";

/// An RNG that yields exactly one caller-chosen 32-byte value.
///
/// `fips204` asks for randomness through `RngCore`; both keygen and hedged
/// signing draw a single 32-byte value. Supplying it directly is how the
/// crate's own API exposes deterministic operation, and it is why a wallet
/// and a node can derive the same key from the same backup.
struct SeedRng {
    seed: [u8; 32],
}

impl RngCore for SeedRng {
    fn next_u32(&mut self) -> u32 {
        unreachable!("ML-DSA draws only whole seeds")
    }
    fn next_u64(&mut self) -> u64 {
        unreachable!("ML-DSA draws only whole seeds")
    }
    fn fill_bytes(&mut self, out: &mut [u8]) {
        let _ = self.try_fill_bytes(out);
    }
    fn try_fill_bytes(&mut self, out: &mut [u8]) -> Result<(), rand_core::Error> {
        if out.len() != self.seed.len() {
            // Refuse rather than pad: a different draw size would mean the
            // library changed what it asks for, and silently obliging would
            // produce keys that no longer match other implementations.
            return Err(rand_core::Error::new("unexpected randomness request size"));
        }
        out.copy_from_slice(&self.seed);
        Ok(())
    }
}

impl CryptoRng for SeedRng {}

fn digest32(domain: &[u8], parts: &[&[u8]]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(domain);
    for part in parts {
        hasher.update(part);
    }
    hasher.finalize().into()
}

/// Decode a hex string, rejecting anything malformed.
fn decode_hex(value: &str, what: &str) -> Result<Vec<u8>, String> {
    hex::decode(value.trim()).map_err(|_| format!("{} is not valid hex", what))
}

/// The ML-DSA seed for an account, derived from its 32-byte master secret.
///
/// One backup, two schemes: the master secret is the private key users
/// already hold, and the post-quantum key is derived from it rather than
/// being a second thing to lose.
pub fn derive_pq_seed(master_secret_hex: &str) -> Result<[u8; 32], String> {
    let master = decode_hex(master_secret_hex, "private key")?;
    if master.len() != 32 {
        return Err("private key must be 32 bytes".to_string());
    }
    Ok(digest32(PQ_SEED_DOMAIN, &[&master]))
}

/// Derive the ML-DSA public key for an account, as hex.
pub fn derive_pq_public_key(master_secret_hex: &str) -> Result<String, String> {
    let (public_key, _) = keypair(master_secret_hex)?;
    Ok(hex::encode(public_key.into_bytes()))
}

fn keypair(
    master_secret_hex: &str,
) -> Result<(ml_dsa_65::PublicKey, ml_dsa_65::PrivateKey), String> {
    let seed = derive_pq_seed(master_secret_hex)?;
    let mut rng = SeedRng { seed };
    ml_dsa_65::KG::try_keygen_with_rng(&mut rng)
        .map_err(|err| format!("ML-DSA key generation failed: {}", err))
}

/// Sign a message with the account's ML-DSA key.
///
/// Deterministic: the same (key, message) always yields the same signature,
/// so a wallet and the CLI produce identical bytes and neither depends on the
/// machine's RNG being sound at signing time.
pub fn sign_message(message: &str, master_secret_hex: &str) -> Result<String, String> {
    let (_, private_key) = keypair(master_secret_hex)?;
    let seed = digest32(PQ_SIGN_DOMAIN, &[&derive_pq_seed(master_secret_hex)?, message.as_bytes()]);
    let signature = private_key
        .try_sign_with_seed(&seed, message.as_bytes(), PQ_CONTEXT)
        .map_err(|err| format!("ML-DSA signing failed: {}", err))?;
    Ok(hex::encode(signature))
}

/// Verify an ML-DSA signature. Returns false for anything malformed rather
/// than erroring, so a caller cannot mistake "could not check" for "valid".
pub fn verify_message(message: &str, public_key_hex: &str, signature_hex: &str) -> bool {
    let Ok(public_bytes) = decode_hex(public_key_hex, "public key") else {
        return false;
    };
    let Ok(signature_bytes) = decode_hex(signature_hex, "signature") else {
        return false;
    };
    let Ok(public_array) = <[u8; PQ_PUBLIC_KEY_LEN]>::try_from(public_bytes.as_slice()) else {
        return false;
    };
    let Ok(signature_array) = <[u8; PQ_SIGNATURE_LEN]>::try_from(signature_bytes.as_slice()) else {
        return false;
    };
    let Ok(public_key) = ml_dsa_65::PublicKey::try_from_bytes(public_array) else {
        return false;
    };
    public_key.verify(message.as_bytes(), &signature_array, PQ_CONTEXT)
}

/// Is this a well-formed ML-DSA-65 public key?
pub fn is_valid_pq_public_key(value: &str) -> bool {
    matches!(decode_hex(value, "public key"), Ok(bytes) if bytes.len() == PQ_PUBLIC_KEY_LEN)
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY_A: &str = "80f91adc283392febbfc86b7327c055b8559373459040e07e78640e3ac592517";
    const KEY_B: &str = "1111111111111111111111111111111111111111111111111111111111111111";

    #[test]
    fn keys_are_derived_deterministically_from_the_master_secret() {
        let first = derive_pq_public_key(KEY_A).unwrap();
        let second = derive_pq_public_key(KEY_A).unwrap();
        assert_eq!(first, second, "same secret produced different keys");
        assert_eq!(first.len(), PQ_PUBLIC_KEY_LEN * 2);
        assert_ne!(derive_pq_public_key(KEY_B).unwrap(), first);
    }

    /// The ML-DSA key must not be the master secret in disguise: it is
    /// domain-separated, so learning one scheme's key tells you nothing about
    /// the other's.
    #[test]
    fn the_pq_seed_is_domain_separated_from_the_master_secret() {
        let seed = derive_pq_seed(KEY_A).unwrap();
        assert_ne!(hex::encode(seed), KEY_A);
    }

    #[test]
    fn signs_and_verifies() {
        let public_key = derive_pq_public_key(KEY_A).unwrap();
        let signature = sign_message("hikmalayer-transfer:a:b:1:1", KEY_A).unwrap();
        assert_eq!(signature.len(), PQ_SIGNATURE_LEN * 2);
        assert!(verify_message("hikmalayer-transfer:a:b:1:1", &public_key, &signature));
    }

    #[test]
    fn signing_is_deterministic() {
        let first = sign_message("same message", KEY_A).unwrap();
        let second = sign_message("same message", KEY_A).unwrap();
        assert_eq!(first, second, "signatures differ across calls");
    }

    #[test]
    fn rejects_a_tampered_message() {
        let public_key = derive_pq_public_key(KEY_A).unwrap();
        let signature = sign_message("pay alice 10", KEY_A).unwrap();
        assert!(!verify_message("pay alice 20", &public_key, &signature));
    }

    #[test]
    fn rejects_another_accounts_key() {
        let other = derive_pq_public_key(KEY_B).unwrap();
        let signature = sign_message("pay alice 10", KEY_A).unwrap();
        assert!(!verify_message("pay alice 10", &other, &signature));
    }

    #[test]
    fn rejects_malformed_input_without_panicking() {
        let public_key = derive_pq_public_key(KEY_A).unwrap();
        let signature = sign_message("m", KEY_A).unwrap();
        assert!(!verify_message("m", "not-hex", &signature));
        assert!(!verify_message("m", &public_key, "not-hex"));
        assert!(!verify_message("m", "", ""));
        assert!(!verify_message("m", &public_key, &signature[..signature.len() - 2]));
        assert!(!verify_message("m", &public_key[..10], &signature));
    }

    #[test]
    fn validates_public_key_shape() {
        assert!(is_valid_pq_public_key(&derive_pq_public_key(KEY_A).unwrap()));
        assert!(!is_valid_pq_public_key(""));
        assert!(!is_valid_pq_public_key("aabb"));
        assert!(!is_valid_pq_public_key("zz"));
    }

    #[test]
    fn rejects_a_master_secret_of_the_wrong_length() {
        assert!(derive_pq_public_key("aabb").is_err());
        assert!(derive_pq_public_key("").is_err());
    }
}
