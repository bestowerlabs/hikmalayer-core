pub mod token;

use http::HeaderMap;
use token::TokenConfig;

/// Representative slice of AppState relevant to R-05.
///
/// In hikmalayer-core's real `state.rs`, replace the existing
/// `admin_token: Option<String>` / `p2p_token: Option<String>` fields with
/// these signing-key fields, and load them via `token::signing_key_from_env`
/// at startup (e.g. `ADMIN_TOKEN_SIGNING_KEY`, `P2P_TOKEN_SIGNING_KEY` as
/// 64-character hex env vars, 32 bytes minimum).
pub struct AppState {
    pub admin_token_signing_key: Option<Vec<u8>>,
    pub p2p_token_signing_key: Option<Vec<u8>>,
}

/// Drop-in replacement for the pre-R-05 `authorize_admin`. Fail-closed
/// (consistent with the HM-04 fix): no signing key configured, missing
/// header, or an invalid/expired token all result in `false`.
pub fn authorize_admin(headers: &HeaderMap, state: &AppState) -> bool {
    let Some(signing_key) = state.admin_token_signing_key.as_ref() else {
        return false;
    };
    let Some(token_str) = headers.get("x-admin-token").and_then(|v| v.to_str().ok()) else {
        return false;
    };

    let cfg = TokenConfig {
        signing_key: signing_key.clone(),
        scope: "admin",
    };
    token::is_valid(&cfg, token_str)
}

/// Drop-in replacement for the pre-R-05 `authorize_p2p`. Same fail-closed
/// semantics as `authorize_admin`.
pub fn authorize_p2p(headers: &HeaderMap, state: &AppState) -> bool {
    let Some(signing_key) = state.p2p_token_signing_key.as_ref() else {
        return false;
    };
    let Some(token_str) = headers.get("x-p2p-token").and_then(|v| v.to_str().ok()) else {
        return false;
    };

    let cfg = TokenConfig {
        signing_key: signing_key.clone(),
        scope: "p2p",
    };
    token::is_valid(&cfg, token_str)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::token::generate_token;

    fn state_with_admin_key(key: Vec<u8>) -> AppState {
        AppState {
            admin_token_signing_key: Some(key),
            p2p_token_signing_key: None,
        }
    }

    fn headers_with(name: &'static str, value: String) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert(name, value.parse().unwrap());
        h
    }

    #[test]
    fn valid_admin_token_is_authorized() {
        let key = vec![0x07; 32];
        let state = state_with_admin_key(key.clone());
        let token = generate_token(
            &TokenConfig {
                signing_key: key,
                scope: "admin",
            },
            300,
        );
        let headers = headers_with("x-admin-token", token);
        assert!(authorize_admin(&headers, &state));
    }

    #[test]
    fn missing_header_is_denied() {
        let state = state_with_admin_key(vec![0x07; 32]);
        let headers = HeaderMap::new();
        assert!(!authorize_admin(&headers, &state));
    }

    #[test]
    fn unconfigured_signing_key_fails_closed() {
        // Mirrors HM-04: no key configured must deny, not allow-all.
        let state = AppState {
            admin_token_signing_key: None,
            p2p_token_signing_key: None,
        };
        let headers = headers_with("x-admin-token", "anything".to_string());
        assert!(!authorize_admin(&headers, &state));
    }

    #[test]
    fn p2p_token_cannot_authorize_admin_endpoint() {
        // A token minted for the p2p scope must not pass admin authorization,
        // even if signed with the same key by mistake.
        let key = vec![0x07; 32];
        let state = state_with_admin_key(key.clone());
        let p2p_token = generate_token(
            &TokenConfig {
                signing_key: key,
                scope: "p2p",
            },
            300,
        );
        let headers = headers_with("x-admin-token", p2p_token);
        assert!(!authorize_admin(&headers, &state));
    }

    #[test]
    fn expired_admin_token_is_denied() {
        let key = vec![0x07; 32];
        let state = state_with_admin_key(key.clone());
        let token = generate_token(
            &TokenConfig {
                signing_key: key,
                scope: "admin",
            },
            0,
        );
        std::thread::sleep(std::time::Duration::from_millis(1100));
        let headers = headers_with("x-admin-token", token);
        assert!(!authorize_admin(&headers, &state));
    }
}
