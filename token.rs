//! HMAC-signed, time-limited tokens for admin and P2P authorization.
//!
//! Addresses R-05 (Residual Risk): ADMIN_TOKEN and P2P_TOKEN were static
//! shared secrets with no expiry and no rotation mechanism.
//!
//! Design:
//!   - Tokens are minted from a long-lived *signing key* (ADMIN_TOKEN_SIGNING_KEY /
//!     P2P_TOKEN_SIGNING_KEY), not distributed as the credential itself.
//!   - Each token embeds an expiry timestamp and is HMAC-SHA256 signed, so it is
//!     self-verifying and naturally expires — satisfying "time-limited tokens".
//!   - Rotation is just minting a new token with `mint_token`; old tokens expire
//!     on their own. Rotating the signing key invalidates all outstanding tokens
//!     immediately for emergency revocation.
//!   - Signature comparison is constant-time, consistent with the HM-07 fix.

use hmac::{Hmac, Mac};
use sha2::Sha256;
use std::time::{SystemTime, UNIX_EPOCH};
use subtle::ConstantTimeEq;

type HmacSha256 = Hmac<Sha256>;

/// Minimum signing key length in bytes. Shorter keys are rejected as
/// misconfiguration rather than silently accepted.
pub const MIN_SIGNING_KEY_LEN: usize = 32;

#[derive(Clone)]
pub struct TokenConfig {
    pub signing_key: Vec<u8>,
    pub scope: &'static str, // "admin" or "p2p"
}

#[derive(Debug, PartialEq, Eq)]
pub enum TokenError {
    Malformed,
    BadSignature,
    Expired,
    ScopeMismatch,
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX epoch")
        .as_secs()
}

/// Mint a new token valid for `ttl_secs` seconds from now.
pub fn generate_token(cfg: &TokenConfig, ttl_secs: u64) -> String {
    let issued_at = now_unix();
    let expires_at = issued_at + ttl_secs;
    let payload = format!("{}:{}:{}", issued_at, expires_at, cfg.scope);
    let payload_b64 = base64_encode(payload.as_bytes());

    let sig = sign(&cfg.signing_key, payload_b64.as_bytes());
    format!("{}.{}", payload_b64, hex::encode(sig))
}

/// Verify a token: signature must be valid for the configured key, the scope
/// must match, and the token must not be expired.
pub fn verify_token(cfg: &TokenConfig, token: &str) -> Result<(), TokenError> {
    let (payload_b64, sig_hex) = token.split_once('.').ok_or(TokenError::Malformed)?;

    let expected_sig = sign(&cfg.signing_key, payload_b64.as_bytes());
    let provided_sig = hex::decode(sig_hex).map_err(|_| TokenError::Malformed)?;

    if expected_sig.ct_eq(&provided_sig).unwrap_u8() != 1 {
        return Err(TokenError::BadSignature);
    }

    let payload_bytes = base64_decode(payload_b64).ok_or(TokenError::Malformed)?;
    let payload = String::from_utf8(payload_bytes).map_err(|_| TokenError::Malformed)?;

    let mut parts = payload.split(':');
    let _issued_at = parts.next().ok_or(TokenError::Malformed)?;
    let expires_at: u64 = parts
        .next()
        .ok_or(TokenError::Malformed)?
        .parse()
        .map_err(|_| TokenError::Malformed)?;
    let scope = parts.next().ok_or(TokenError::Malformed)?;

    if scope != cfg.scope {
        return Err(TokenError::ScopeMismatch);
    }
    if now_unix() >= expires_at {
        return Err(TokenError::Expired);
    }

    Ok(())
}

/// Convenience wrapper returning a bool, for drop-in use in `authorize_*`
/// functions that previously did a plain string comparison.
pub fn is_valid(cfg: &TokenConfig, token: &str) -> bool {
    verify_token(cfg, token).is_ok()
}

fn sign(key: &[u8], message: &[u8]) -> Vec<u8> {
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC accepts any key length");
    mac.update(message);
    mac.finalize().into_bytes().to_vec()
}

/// Load a signing key from an env var, hex-encoded. Returns `None` (treated
/// as "unconfigured") if the var is unset, empty, not valid hex, or shorter
/// than `MIN_SIGNING_KEY_LEN` bytes once decoded — mirrors the HM-04 fail-closed
/// pattern: misconfiguration must deny, never silently allow.
pub fn signing_key_from_env(var_name: &str) -> Option<Vec<u8>> {
    let raw = std::env::var(var_name).ok()?;
    if raw.is_empty() {
        return None;
    }
    let decoded = hex::decode(raw.trim()).ok()?;
    if decoded.len() < MIN_SIGNING_KEY_LEN {
        return None;
    }
    Some(decoded)
}

fn base64_encode(input: &[u8]) -> String {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
    URL_SAFE_NO_PAD.encode(input)
}

fn base64_decode(input: &str) -> Option<Vec<u8>> {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
    URL_SAFE_NO_PAD.decode(input).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(scope: &'static str) -> TokenConfig {
        TokenConfig {
            signing_key: vec![0x42; 32],
            scope,
        }
    }

    #[test]
    fn valid_token_round_trips() {
        let c = cfg("admin");
        let token = generate_token(&c, 60);
        assert!(is_valid(&c, &token));
    }

    #[test]
    fn expired_token_is_rejected() {
        let c = cfg("admin");
        // ttl 0 -> expires_at == issued_at, so "now >= expires_at" immediately
        let token = generate_token(&c, 0);
        std::thread::sleep(std::time::Duration::from_millis(1100));
        assert_eq!(verify_token(&c, &token), Err(TokenError::Expired));
    }

    #[test]
    fn wrong_scope_is_rejected() {
        let admin_cfg = cfg("admin");
        let p2p_cfg = cfg("p2p");
        let token = generate_token(&admin_cfg, 60);
        // same key, different scope
        assert_eq!(
            verify_token(&p2p_cfg, &token),
            Err(TokenError::ScopeMismatch)
        );
    }

    #[test]
    fn tampered_signature_is_rejected() {
        let c = cfg("admin");
        let token = generate_token(&c, 60);
        let (payload, _sig) = token.split_once('.').unwrap();
        let tampered = format!("{}.{}", payload, "00".repeat(32));
        assert_eq!(verify_token(&c, &tampered), Err(TokenError::BadSignature));
    }

    #[test]
    fn tampered_payload_is_rejected() {
        let c = cfg("admin");
        let token = generate_token(&c, 60);
        let (_payload, sig) = token.split_once('.').unwrap();
        let forged_payload = base64_encode(b"0:9999999999:admin");
        let tampered = format!("{}.{}", forged_payload, sig);
        assert_eq!(verify_token(&c, &tampered), Err(TokenError::BadSignature));
    }

    #[test]
    fn wrong_signing_key_is_rejected() {
        let c1 = cfg("admin");
        let mut c2 = cfg("admin");
        c2.signing_key = vec![0x99; 32];
        let token = generate_token(&c1, 60);
        assert_eq!(verify_token(&c2, &token), Err(TokenError::BadSignature));
    }

    #[test]
    fn malformed_token_is_rejected() {
        let c = cfg("admin");
        assert_eq!(verify_token(&c, "not-a-token"), Err(TokenError::Malformed));
        assert_eq!(verify_token(&c, ""), Err(TokenError::Malformed));
    }

    #[test]
    fn signing_key_from_env_rejects_short_or_missing() {
        std::env::remove_var("HM_TEST_KEY_UNSET");
        assert_eq!(signing_key_from_env("HM_TEST_KEY_UNSET"), None);

        std::env::set_var("HM_TEST_KEY_SHORT", "aabb"); // 2 bytes, below 32-byte minimum
        assert_eq!(signing_key_from_env("HM_TEST_KEY_SHORT"), None);
        std::env::remove_var("HM_TEST_KEY_SHORT");

        let good = hex::encode([0x11u8; 32]);
        std::env::set_var("HM_TEST_KEY_GOOD", &good);
        assert_eq!(signing_key_from_env("HM_TEST_KEY_GOOD"), Some(vec![0x11; 32]));
        std::env::remove_var("HM_TEST_KEY_GOOD");
    }
}
