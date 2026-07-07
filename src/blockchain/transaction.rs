use chrono::prelude::*;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::consensus::pos;

/// Reward minted to the block validator for each accepted block.
pub const BLOCK_REWARD: u64 = 5;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TransactionType {
    Transfer,    // Transfer tokens
    Reward,      // PoS or PoW reward
    Certificate, // Issue certificate
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction {
    pub id: String,
    pub from: Option<String>, // None for rewards
    pub to: String,
    pub amount: u64,
    pub transaction_type: TransactionType,
    pub timestamp: DateTime<Utc>,
    /// Strictly increasing per-account nonce for replay protection.
    #[serde(default)]
    pub nonce: u64,
    /// Sender public key (hex, uncompressed secp256k1) for Transfer txs.
    #[serde(default)]
    pub public_key: Option<String>,
    /// Compact ECDSA signature over `transfer_signing_message`.
    #[serde(default)]
    pub signature: Option<String>,
}

impl Transaction {
    pub fn new(
        from: Option<String>,
        to: String,
        amount: u64,
        transaction_type: TransactionType,
    ) -> Self {
        Transaction {
            id: Uuid::new_v4().to_string(),
            from,
            to,
            amount,
            transaction_type,
            timestamp: Utc::now(),
            nonce: 0,
            public_key: None,
            signature: None,
        }
    }

    /// The block reward paid to the validator that produced the block.
    pub fn new_reward(validator: &str) -> Self {
        Self::new(None, validator.to_string(), BLOCK_REWARD, TransactionType::Reward)
    }

    /// Canonical message a sender signs to authorize a transfer. Includes the
    /// nonce so a signature can never be replayed.
    pub fn transfer_signing_message(from: &str, to: &str, amount: u64, nonce: u64) -> String {
        format!("hikmalayer-transfer:{}:{}:{}:{}", from, to, amount, nonce)
    }

    /// Consensus-level verification of a transaction inside a block produced
    /// by `validator`.
    pub fn verify_for_block(&self, validator: &str) -> Result<(), String> {
        match self.transaction_type {
            TransactionType::Transfer => {
                let from = self
                    .from
                    .as_ref()
                    .ok_or_else(|| "Transfer transaction missing sender".to_string())?;
                let signature = self
                    .signature
                    .as_ref()
                    .ok_or_else(|| "Transfer transaction missing signature".to_string())?;

                let message =
                    Self::transfer_signing_message(from, &self.to, self.amount, self.nonce);
                if !verify_transfer_signature(from, &message, self.public_key.as_deref(), signature)
                {
                    return Err("Transfer signature verification failed".to_string());
                }
                Ok(())
            }
            TransactionType::Reward => {
                if self.from.is_some() {
                    return Err("Reward transaction must not have a sender".to_string());
                }
                if self.to != validator {
                    return Err("Reward transaction must pay the block validator".to_string());
                }
                if self.amount != BLOCK_REWARD {
                    return Err("Reward transaction has invalid amount".to_string());
                }
                Ok(())
            }
            TransactionType::Certificate => {
                if self.amount != 0 {
                    return Err("Certificate transaction must not carry value".to_string());
                }
                Ok(())
            }
        }
    }
}

/// Verify a transfer authorization in either supported scheme:
///
/// * **Raw secp256k1** (hikma-wallet / validator tooling): 64-byte compact
///   signature over SHA-256 of the message, verified against `public_key`,
///   whose derived address must be the sender.
/// * **Ethereum personal_sign** (MetaMask / browser wallets): 65-byte
///   recoverable signature; the recovered address must be the sender.
pub fn verify_transfer_signature(
    from: &str,
    message: &str,
    public_key: Option<&str>,
    signature: &str,
) -> bool {
    let sig_hex = signature.strip_prefix("0x").unwrap_or(signature);
    let sig_len = hex::decode(sig_hex).map(|bytes| bytes.len()).unwrap_or(0);

    if sig_len == 65 {
        return crate::auth::signature::verify_signature(from, message, signature);
    }

    let Some(public_key) = public_key else {
        return false;
    };
    let Ok(derived) = pos::derive_address(public_key) else {
        return false;
    };
    if derived.to_lowercase() != from.to_lowercase() {
        return false;
    }
    pos::verify_message(message, public_key, signature)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transaction_creation() {
        let tx = Transaction::new(
            Some("Alice".to_string()),
            "Bob".to_string(),
            100,
            TransactionType::Transfer,
        );
        assert_eq!(tx.amount, 100);
        assert_eq!(tx.to, "Bob");
    }

    #[test]
    fn reward_verification_enforces_recipient_and_amount() {
        let reward = Transaction::new_reward("validator-1");
        assert!(reward.verify_for_block("validator-1").is_ok());
        assert!(reward.verify_for_block("validator-2").is_err());

        let mut inflated = Transaction::new_reward("validator-1");
        inflated.amount = BLOCK_REWARD + 100;
        assert!(inflated.verify_for_block("validator-1").is_err());
    }

    #[test]
    fn transfer_verification_supports_personal_sign() {
        use secp256k1::{Message, Secp256k1, SecretKey};
        use sha3::{Digest, Keccak256};

        let secp = Secp256k1::new();
        let secret_key = SecretKey::from_slice(&[5u8; 32]).unwrap();
        let public_key = secret_key.public_key(&secp);
        let uncompressed = public_key.serialize_uncompressed();
        let address_hash = Keccak256::digest(&uncompressed[1..]);
        let from = format!("0x{}", hex::encode(&address_hash[12..]));

        let message = Transaction::transfer_signing_message(&from, "0xdest", 7, 1);
        let prefixed = format!("\x19Ethereum Signed Message:\n{}{}", message.len(), message);
        let digest = Keccak256::digest(prefixed.as_bytes());
        let msg = Message::from_digest_slice(&digest).unwrap();
        let recoverable = secp.sign_ecdsa_recoverable(&msg, &secret_key);
        let (recovery_id, compact) = recoverable.serialize_compact();
        let mut signature_bytes = [0u8; 65];
        signature_bytes[..64].copy_from_slice(&compact);
        signature_bytes[64] = (recovery_id.to_i32() as u8) + 27;
        let signature = format!("0x{}", hex::encode(signature_bytes));

        assert!(verify_transfer_signature(&from, &message, None, &signature));
        assert!(!verify_transfer_signature(
            "0x0000000000000000000000000000000000000000",
            &message,
            None,
            &signature
        ));
    }

    #[test]
    fn transfer_verification_requires_valid_signature() {
        let private_key = hex::encode([3u8; 32]);
        let public_key = pos::derive_public_key(&private_key).unwrap();
        let from = pos::derive_address(&public_key).unwrap();

        let mut tx = Transaction::new(
            Some(from.clone()),
            "0xrecipient".to_string(),
            42,
            TransactionType::Transfer,
        );
        tx.nonce = 1;
        tx.public_key = Some(public_key.clone());

        // Unsigned: rejected.
        assert!(tx.verify_for_block("validator-1").is_err());

        let message = Transaction::transfer_signing_message(&from, &tx.to, tx.amount, tx.nonce);
        tx.signature = Some(pos::sign_message(&message, &private_key).unwrap());
        assert!(tx.verify_for_block("validator-1").is_ok());

        // Tampered amount: rejected.
        tx.amount = 9999;
        assert!(tx.verify_for_block("validator-1").is_err());
    }
}
