use chrono::prelude::*;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::blockchain::block::Block;
use crate::consensus::pos;

/// Reward minted to the block validator for each accepted block.
pub const BLOCK_REWARD: u64 = 5;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TransactionType {
    Transfer,    // Transfer tokens
    Reward,      // Block production reward
    Certificate, // Anchor a certificate issuance
    Stake,       // Register / increase validator stake (on-chain)
    Withdraw,    // Reduce / exit validator stake (on-chain)
    Slash,       // Punish a proven equivocation (on-chain)
}

/// Proof that one validator signed two different blocks at the same height.
/// Both blocks are self-contained: their hashes are recomputable from the
/// header fields, and each carries the validator's signature over its hash.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlashProof {
    pub block_a: Block,
    pub block_b: Block,
}

impl SlashProof {
    /// Purely cryptographic (stateless) verification of the equivocation.
    /// Whether the key is the validator's *registered* key is checked
    /// statefully when the slash transaction is applied.
    pub fn verify(&self) -> Result<String, String> {
        let a = &self.block_a;
        let b = &self.block_b;

        if a.index != b.index {
            return Err("Equivocation proof blocks are at different heights".to_string());
        }
        if a.hash == b.hash {
            return Err("Equivocation proof blocks are identical".to_string());
        }

        let validator = a
            .validator
            .clone()
            .ok_or_else(|| "Proof block missing validator".to_string())?;
        if b.validator.as_deref() != Some(validator.as_str()) {
            return Err("Proof blocks have different validators".to_string());
        }

        let key_a = a
            .validator_public_key
            .as_ref()
            .ok_or_else(|| "Proof block missing public key".to_string())?;
        let key_b = b
            .validator_public_key
            .as_ref()
            .ok_or_else(|| "Proof block missing public key".to_string())?;
        if key_a != key_b {
            return Err("Proof blocks signed with different keys".to_string());
        }

        // Hashes must be honestly derived from the header fields.
        if a.hash != a.calculate_hash() || b.hash != b.calculate_hash() {
            return Err("Proof block hash does not match its header".to_string());
        }

        for block in [a, b] {
            let signature = block
                .validator_signature
                .as_ref()
                .ok_or_else(|| "Proof block missing signature".to_string())?;
            if !pos::verify_block_signature(&block.hash, key_a, signature) {
                return Err("Proof block signature verification failed".to_string());
            }
        }

        Ok(validator)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction {
    pub id: String,
    pub from: Option<String>, // None for rewards / slashes
    pub to: String,
    pub amount: u64,
    pub transaction_type: TransactionType,
    pub timestamp: DateTime<Utc>,
    /// Strictly increasing per-account nonce for replay protection.
    #[serde(default)]
    pub nonce: u64,
    /// Sender public key (hex, uncompressed secp256k1).
    #[serde(default)]
    pub public_key: Option<String>,
    /// Compact ECDSA signature over the transaction's signing message.
    #[serde(default)]
    pub signature: Option<String>,
    /// Equivocation proof (Slash transactions only).
    #[serde(default)]
    pub slash_proof: Option<SlashProof>,
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
            slash_proof: None,
        }
    }

    /// The block reward paid to the validator that produced the block.
    pub fn new_reward(validator: &str) -> Self {
        Self::new(None, validator.to_string(), BLOCK_REWARD, TransactionType::Reward)
    }

    /// Canonical message a sender signs to authorize a transfer.
    pub fn transfer_signing_message(from: &str, to: &str, amount: u64, nonce: u64) -> String {
        format!("hikmalayer-transfer:{}:{}:{}:{}", from, to, amount, nonce)
    }

    /// Canonical message a validator signs to authorize a stake deposit.
    pub fn stake_signing_message(address: &str, amount: u64, nonce: u64) -> String {
        format!("hikmalayer-stake:{}:{}:{}", address, amount, nonce)
    }

    /// Canonical message a validator signs to authorize a stake withdrawal.
    pub fn withdraw_signing_message(address: &str, amount: u64, nonce: u64) -> String {
        format!("hikmalayer-withdraw:{}:{}:{}", address, amount, nonce)
    }

    /// Stateless consensus verification of a transaction inside a block
    /// produced by `validator`. Stateful rules (nonces, balances, registered
    /// keys) are enforced by `ChainState::apply_transaction`.
    pub fn verify_for_block(&self, validator: &str) -> Result<(), String> {
        match self.transaction_type {
            TransactionType::Transfer => {
                let from = self
                    .from
                    .as_ref()
                    .ok_or_else(|| "Transfer transaction missing sender".to_string())?;
                let message =
                    Self::transfer_signing_message(from, &self.to, self.amount, self.nonce);
                self.verify_sender_signature(from, &message)
            }
            TransactionType::Stake => {
                let from = self
                    .from
                    .as_ref()
                    .ok_or_else(|| "Stake transaction missing sender".to_string())?;
                if self.to != crate::blockchain::state::STAKING_POOL_ACCOUNT {
                    return Err("Stake transaction must pay the staking pool".to_string());
                }
                if self.amount == 0 {
                    return Err("Stake amount must be greater than zero".to_string());
                }
                let message = Self::stake_signing_message(from, self.amount, self.nonce);
                self.verify_sender_signature(from, &message)
            }
            TransactionType::Withdraw => {
                // Signature is verified against the ON-CHAIN registered key
                // when the transaction is applied; here we check structure.
                if self.from.is_none() {
                    return Err("Withdraw transaction missing sender".to_string());
                }
                if self.signature.is_none() {
                    return Err("Withdraw transaction missing signature".to_string());
                }
                if self.amount == 0 {
                    return Err("Withdraw amount must be greater than zero".to_string());
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
            TransactionType::Slash => {
                if self.from.is_some() || self.amount != 0 {
                    return Err("Slash transaction must carry no sender or value".to_string());
                }
                let proof = self
                    .slash_proof
                    .as_ref()
                    .ok_or_else(|| "Slash transaction missing proof".to_string())?;
                let offender = proof.verify()?;
                if self.to != offender {
                    return Err("Slash transaction target does not match proof".to_string());
                }
                Ok(())
            }
        }
    }

    /// Verify a native Hikmalayer signature: the embedded public key must
    /// derive to the sender's address and the compact secp256k1 signature
    /// must verify over the domain-prefixed message.
    fn verify_sender_signature(&self, from: &str, message: &str) -> Result<(), String> {
        let public_key = self
            .public_key
            .as_ref()
            .ok_or_else(|| "Transaction missing public key".to_string())?;
        let signature = self
            .signature
            .as_ref()
            .ok_or_else(|| "Transaction missing signature".to_string())?;

        let derived = pos::derive_address(public_key)?;
        if derived != *from {
            return Err("Sender address does not match the signing key".to_string());
        }
        if !pos::verify_message(message, public_key, signature) {
            return Err("Transaction signature verification failed".to_string());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wallet(seed: u8) -> (String, String, String) {
        let private_key = hex::encode([seed; 32]);
        let public_key = pos::derive_public_key(&private_key).unwrap();
        let address = pos::derive_address(&public_key).unwrap();
        (address, public_key, private_key)
    }

    #[test]
    fn test_transaction_creation() {
        let tx = Transaction::new(
            Some("hkmalice".to_string()),
            "hkmbob".to_string(),
            100,
            TransactionType::Transfer,
        );
        assert_eq!(tx.amount, 100);
        assert_eq!(tx.to, "hkmbob");
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
    fn transfer_verification_requires_valid_native_signature() {
        let (from, public_key, private_key) = wallet(3);

        let mut tx = Transaction::new(
            Some(from.clone()),
            "hkmrecipient".to_string(),
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

    #[test]
    fn transfer_rejects_key_not_matching_sender() {
        let (_, public_key, private_key) = wallet(4);
        let (victim, ..) = wallet(5);

        let mut tx = Transaction::new(
            Some(victim.clone()),
            "hkmattacker".to_string(),
            42,
            TransactionType::Transfer,
        );
        tx.nonce = 1;
        tx.public_key = Some(public_key);
        let message = Transaction::transfer_signing_message(&victim, &tx.to, tx.amount, tx.nonce);
        tx.signature = Some(pos::sign_message(&message, &private_key).unwrap());
        assert!(tx.verify_for_block("validator-1").is_err());
    }

    #[test]
    fn stake_verification_binds_pool_and_signature() {
        let (from, public_key, private_key) = wallet(6);
        let mut tx = Transaction::new(
            Some(from.clone()),
            crate::blockchain::state::STAKING_POOL_ACCOUNT.to_string(),
            100,
            TransactionType::Stake,
        );
        tx.nonce = 1;
        tx.public_key = Some(public_key);
        let message = Transaction::stake_signing_message(&from, 100, 1);
        tx.signature = Some(pos::sign_message(&message, &private_key).unwrap());
        assert!(tx.verify_for_block("validator-1").is_ok());

        // Wrong destination account: rejected.
        tx.to = "hkmsomewhere".to_string();
        assert!(tx.verify_for_block("validator-1").is_err());
    }
}
