//! R-05 remediation: authorization entry points that verify HMAC-signed,
//! self-expiring tokens instead of comparing against a static shared secret.
//!
//! These are drop-in replacements for the existing `authorize_admin()` /
//! `authorize_p2p()` used in routes.rs — same `HeaderMap` in, `bool` out
//! signature, so the six gated handlers from HM-02 don't need to change.

pub mod token;

use axum::http::HeaderMap;
use token::{is_valid, Scope};

/// Fail-closed: returns false if the signing key is unset/malformed, the
/// Authorization header is missing, or the token is expired, mis-scoped,
/// or fails signature verification. Never falls back to allowing access.
pub fn authorize_admin(headers: &HeaderMap, signing_key: &Option<Vec<u8>>) -> bool {
    authorize(headers, signing_key, Scope::Admin)
}

pub fn authorize_p2p(headers: &HeaderMap, signing_key: &Option<Vec<u8>>) -> bool {
    authorize(headers, signing_key, Scope::P2p)
}

fn authorize(headers: &HeaderMap, signing_key: &Option<Vec<u8>>, scope: Scope) -> bool {
    let key = match signing_key {
        Some(k) => k,
        None => return false, // unconfigured key must never authorize
    };

    let token = match headers.get("Authorization").and_then(|v| v.to_str().ok()) {
        Some(raw) => raw.trim_start_matches("Bearer ").to_string(),
        None => return false,
    };

    is_valid(&token, key, scope)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;
    use token::generate_token;

    fn headers_with_token(t: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert("Authorization", HeaderValue::from_str(&format!("Bearer {}", t)).unwrap());
        headers
    }

    #[test]
    fn authorizes_valid_admin_token() {
        let key = vec![0x11; 32];
        let token = generate_token(&key, Scope::Admin, 3600);
        assert!(authorize_admin(&headers_with_token(&token), &Some(key)));
    }

    #[test]
    fn denies_missing_header() {
        let key = vec![0x11; 32];
        assert!(!authorize_admin(&HeaderMap::new(), &Some(key)));
    }

    #[test]
    fn denies_unconfigured_signing_key() {
        let token = generate_token(&vec![0x11; 32], Scope::Admin, 3600);
        assert!(!authorize_admin(&headers_with_token(&token), &None));
    }

    #[test]
    fn denies_p2p_token_on_admin_route() {
        let key = vec![0x11; 32];
        let token = generate_token(&key, Scope::P2p, 3600);
        assert!(!authorize_admin(&headers_with_token(&token), &Some(key)));
    }

    #[test]
    fn denies_expired_token() {
        let key = vec![0x11; 32];
        let token = generate_token(&key, Scope::Admin, 0);
        std::thread::sleep(std::time::Duration::from_secs(1));
        assert!(!authorize_admin(&headers_with_token(&token), &Some(key)));
    }
}
