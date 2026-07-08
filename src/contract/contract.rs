use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Represents a simple on-chain certificate
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Certificate {
    pub id: String,
    pub issued_to: String,
    pub description: String,
    pub verified: bool,
}

/// The contract executor that manages certificates and token issuance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractExecutor {
    pub certificates: HashMap<String, Certificate>,
}

impl ContractExecutor {
    pub fn new() -> Self {
        ContractExecutor {
            certificates: HashMap::new(),
        }
    }

    /// Issues a new certificate
    pub fn issue_certificate(&mut self, id: &str, issued_to: &str, description: &str) {
        let cert = Certificate {
            id: id.to_string(),
            issued_to: issued_to.to_string(),
            description: description.to_string(),
            verified: false,
        };
        self.certificates.insert(id.to_string(), cert);
        println!("Certificate {} issued to {}", id, issued_to);
    }

    /// Verifies a certificate by ID
    pub fn verify_certificate(&mut self, id: &str) -> bool {
        if let Some(cert) = self.certificates.get_mut(id) {
            cert.verified = true;
            println!("Certificate {} verified!", id);
            true
        } else {
            println!("Certificate {} not found.", id);
            false
        }
    }

}
