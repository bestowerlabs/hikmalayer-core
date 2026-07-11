// src/auth/signature.rs
//
// Native Hikmalayer signature verification for the session-auth flow.
// No external chain conventions: addresses are hkm… (SHA-256 derived) and
// messages are signed under the Hikmalayer signing domain (see
// consensus::pos::MESSAGE_PREFIX).

use crate::consensus::pos;

/// Verify that `signature` over `message` was produced by the key behind
/// `address`. The caller supplies the public key; it must derive to the
/// claimed address and the compact secp256k1 signature must verify under
/// the native signing domain.
pub fn verify_signature(
    address: &str,
    message: &str,
    public_key_hex: &str,
    signature_hex: &str,
) -> bool {
    match pos::derive_address(public_key_hex) {
        Ok(derived) if derived == address => {
            pos::verify_message(message, public_key_hex, signature_hex)
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verifies_native_signatures_and_rejects_mismatches() {
        let private_key = hex::encode([8u8; 32]);
        let public_key = pos::derive_public_key(&private_key).unwrap();
        let address = pos::derive_address(&public_key).unwrap();
        let message = "login-nonce-1234";
        let signature = pos::sign_message(message, &private_key).unwrap();

        assert!(verify_signature(&address, message, &public_key, &signature));
        assert!(!verify_signature(
            "hkm0000000000000000000000000000000000000000",
            message,
            &public_key,
            &signature
        ));
        assert!(!verify_signature(&address, "other", &public_key, &signature));
    }
}
