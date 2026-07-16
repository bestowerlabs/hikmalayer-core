// src/auth/mod.rs
pub mod middleware;
pub mod routes;
pub mod signature;
pub mod token;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub struct AuthManager {
    pub sessions: HashMap<String, String>,
    pub nonces: HashMap<String, String>,
}

impl AuthManager {
    pub fn new() -> Self {
        Self {
            sessions: HashMap::new(),
            nonces: HashMap::new(),
        }
    }

    pub fn generate_nonce(&mut self, address: &str) -> String {
        let nonce = uuid::Uuid::new_v4().to_string();
        self.nonces.insert(address.to_string(), nonce.clone());
        nonce
    }

    pub fn verify_nonce(&self, address: &str, nonce: &str) -> bool {
        self.nonces
            .get(address)
            .map_or(false, |stored_nonce| stored_nonce == nonce)
    }

    pub fn create_session(&mut self, address: &str) -> String {
        let token = format!("{}_{}", uuid::Uuid::new_v4().to_string(), address);
        self.sessions.insert(token.clone(), address.to_string());
        // Remove the nonce after successful authentication
        self.nonces.remove(address);
        token
    }

    pub fn verify_session(&self, token: &str) -> Option<String> {
        self.sessions.get(token).cloned()
    }

    pub fn revoke_session(&mut self, token: &str) {
        self.sessions.remove(token);
    }
}

#[derive(Deserialize)]
pub struct NonceRequest {
    pub address: String,
}

#[derive(Serialize)]
pub struct NonceResponse {
    pub nonce: String,
}

#[derive(Deserialize)]
pub struct VerifyRequest {
    pub address: String,
    pub message: String,
    pub public_key: String,
    pub signature: String,
    pub nonce: String,
}

#[derive(Serialize)]
pub struct VerifyResponse {
    pub token: String,
    pub address: String,
}

#[derive(Serialize)]
pub struct AuthResponse {
    pub status: String,
    pub message: String,
}
