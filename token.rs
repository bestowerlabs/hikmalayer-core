//! R-05 remediation: HMAC-signed, self-expiring admin/P2P tokens.
//!
//! Token format: base64(issued_at:expires_at:scope) . hex(hmac_sha256(payload))
//!
//! Replaces the static ADMIN_TOKEN / P2P_TOKEN shared secrets with tokens minted
//! from a long-lived signing key. Verification is stateless (no DB/session store),
//! fail-closed if the signing key is missing/malformed, and uses constant-time
//! signature comparison to avoid timing side channels.

use hmac::{Hmac, Mac};
use sha2::Sha256;
use std::time::{SystemTime, UNIX_EPOCH};
use subtle::ConstantTimeEq;

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Admin,
    P2p,
}

impl Scope {
    fn as_str(&self) -> &'static str {
        match self {
            Scope::Admin => "admin",
            Scope::P2p => "p2p",
        }
    }

    fn from_str(s: &str) -> Option<Self> {
        match s {
            "admin" => Some(Scope::Admin),
            "p2p" => Some(Scope::P2p),
            _ => None,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum TokenError {
    MalformedToken,
    InvalidSignature,
    Expired,
    ScopeMismatch,
}

/// Loads a signing key from an env var containing a 64-character hex string
/// (e.g. generated via `openssl rand -hex 32`). Returns None if unset or malformed,
/// so callers can fail closed rather than falling back to a default key.
pub fn signing_key_from_env(var_name: &str) -> Option<Vec<u8>> {
    let hex_key = std::env::var(var_name).ok()?;
    if hex_key.len() != 64 {
        return None;
    }
    hex::decode(hex_key).ok()
}

fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before epoch")
        .as_secs()
}

/// Mints a new signed token for the given scope with a TTL in seconds.
/// This is the rotation mechanism: calling this again produces a fresh token
/// without needing to redeploy or restart the app.
pub fn generate_token(signing_key: &[u8], scope: Scope, ttl_seconds: u64) -> String {
    let issued_at = current_timestamp();
    let expires_at = issued_at + ttl_seconds;
    let payload = format!("{}:{}:{}", issued_at, expires_at, scope.as_str());

    let mut mac = HmacSha256::new_from_slice(signing_key).expect("HMAC accepts any key length");
    mac.update(payload.as_bytes());
    let signature = mac.finalize().into_bytes();

    let b64_payload = base64::encode(&payload);
    let hex_sig = hex::encode(signature);

    format!("{}.{}", b64_payload, hex_sig)
}

/// Verifies a token: signature integrity, scope binding, and expiry.
/// Rejects on any tamper, mismatch, or expiry — never fails open.
pub fn verify_token(token: &str, signing_key: &[u8], expected_scope: Scope) -> Result<(), TokenError> {
    let (b64_payload, hex_sig) = token.split_once('.').ok_or(TokenError::MalformedToken)?;

    let payload_bytes = base64::decode(b64_payload).map_err(|_| TokenError::MalformedToken)?;
    let payload = String::from_utf8(payload_bytes).map_err(|_| TokenError::MalformedToken)?;
    let sig_bytes = hex::decode(hex_sig).map_err(|_| TokenError::MalformedToken)?;

    let mut mac = HmacSha256::new_from_slice(signing_key).expect("HMAC accepts any key length");
    mac.update(payload.as_bytes());
    let expected_sig = mac.finalize().into_bytes();

    // Constant-time comparison — same timing-side-channel mitigation as HM-07,
    // rather than a plain `==` string check.
    if expected_sig.as_slice().ct_eq(&sig_bytes).unwrap_u8() != 1 {
        return Err(TokenError::InvalidSignature);
    }

    let mut parts = payload.splitn(3, ':');
    let _issued_at: u64 = parts
        .next()
        .ok_or(TokenError::MalformedToken)?
        .parse()
        .map_err(|_| TokenError::MalformedToken)?;
    let expires_at: u64 = parts
        .next()
        .ok_or(TokenError::MalformedToken)?
        .parse()
        .map_err(|_| TokenError::MalformedToken)?;
    let scope_str = parts.next().ok_or(TokenError::MalformedToken)?;

    let token_scope = Scope::from_str(scope_str).ok_or(TokenError::MalformedToken)?;
    if token_scope != expected_scope {
        return Err(TokenError::ScopeMismatch);
    }

    if current_timestamp() > expires_at {
        return Err(TokenError::Expired);
    }

    Ok(())
}

/// Convenience boolean wrapper around `verify_token` for call sites that
/// don't need the specific failure reason (e.g. authorize_admin/authorize_p2p).
pub fn is_valid(token: &str, signing_key: &[u8], expected_scope: Scope) -> bool {
    verify_token(token, signing_key, expected_scope).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_key() -> Vec<u8> {
        vec![0x42; 32]
    }

    #[test]
    fn valid_round_trip() {
        let key = test_key();
        let token = generate_token(&key, Scope::Admin, 3600);
        assert!(verify_token(&token, &key, Scope::Admin).is_ok());
    }

    #[test]
    fn expired_token_rejected() {
        let key = test_key();
        let token = generate_token(&key, Scope::Admin, 0);
        std::thread::sleep(std::time::Duration::from_secs(1));
        assert_eq!(verify_token(&token, &key, Scope::Admin), Err(TokenError::Expired));
    }

    #[test]
    fn scope_mismatch_rejected() {
        let key = test_key();
        let token = generate_token(&key, Scope::Admin, 3600);
        assert_eq!(verify_token(&token, &key, Scope::P2p), Err(TokenError::ScopeMismatch));
    }

    #[test]
    fn tampered_signature_rejected() {
        let key = test_key();
        let token = generate_token(&key, Scope::Admin, 3600);
        let mut parts: Vec<&str> = token.split('.').collect();
        let bad_sig = "0".repeat(parts[1].len());
        parts[1] = &bad_sig;
        let tampered = parts.join(".");
        assert_eq!(verify_token(&tampered, &key, Scope::Admin), Err(TokenError::InvalidSignature));
    }

    #[test]
    fn tampered_payload_rejected() {
        let key = test_key();
        let token = generate_token(&key, Scope::Admin, 3600);
        let parts: Vec<&str> = token.split('.').collect();
        let forged_payload = base64::encode("0:9999999999:admin");
        let tampered = format!("{}.{}", forged_payload, parts[1]);
        assert_eq!(verify_token(&tampered, &key, Scope::Admin), Err(TokenError::InvalidSignature));
    }

    #[test]
    fn wrong_signing_key_rejected() {
        let key = test_key();
        let other_key = vec![0x24; 32];
        let token = generate_token(&key, Scope::Admin, 3600);
        assert_eq!(verify_token(&token, &other_key, Scope::Admin), Err(TokenError::InvalidSignature));
    }

    #[test]
    fn malformed_token_rejected() {
        let key = test_key();
        assert_eq!(
            verify_token("not-a-valid-token", &key, Scope::Admin),
            Err(TokenError::MalformedToken)
        );
    }

    #[test]
    fn signing_key_from_env_missing_or_bad_length() {
        std::env::remove_var("TEST_R05_MISSING_KEY");
        assert!(signing_key_from_env("TEST_R05_MISSING_KEY").is_none());

        std::env::set_var("TEST_R05_SHORT_KEY", "abcd");
        assert!(signing_key_from_env("TEST_R05_SHORT_KEY").is_none());
        std::env::remove_var("TEST_R05_SHORT_KEY");
    }
}
