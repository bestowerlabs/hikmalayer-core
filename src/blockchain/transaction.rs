use chrono::prelude::*;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::blockchain::block::Block;
use crate::consensus::pos;

/// HKM is denominated with 6 decimal places: all on-chain amounts are in
/// base units, and 1 HKM = 1,000,000 base units. Chosen so the ~100B HKM
/// supply (10^17 base units) keeps ~180x headroom under u64::MAX — no
/// balance or supply aggregate can overflow.
pub const DECIMALS: u32 = 6;
pub const UNITS_PER_HKM: u64 = 1_000_000;

/// Initial block reward: 5,000 HKM (height 1 through the first halving).
pub const BLOCK_REWARD: u64 = 5_000 * UNITS_PER_HKM;

/// Blocks between reward halvings — a Bitcoin-style deterministic emission
/// schedule. At the 15s block target, 8,000,000 blocks ≈ 3.8 years per
/// halving epoch (Bitcoin cadence). Halving-phase emission sums to just
/// under 80B HKM, which with the 20B HKM genesis allocation gives the
/// ~100B HKM supply at maturity.
pub const HALVING_INTERVAL: u64 = 8_000_000;

/// Tail emission floor: 50 HKM per block. Once halvings would push the
/// reward below this floor (epoch 8, ~29 years in), the reward stays here
/// forever — a perpetual security budget (~0.1%/year of the 100B supply,
/// a rate that decays as supply grows) so validators are never left with
/// fees alone. Monero-style: supply is asymptotically capped in *rate*,
/// not absolute count.
pub const TAIL_EMISSION: u64 = 50 * UNITS_PER_HKM;

/// Deterministic block reward for the block at `height`. Genesis (height 0)
/// pays nothing; every subsequent block pays `BLOCK_REWARD >> halvings`
/// (where `halvings = (height - 1) / HALVING_INTERVAL`) floored at
/// TAIL_EMISSION. Every node computes the identical schedule, so emission
/// is consensus-enforced.
pub fn block_reward(height: u64) -> u64 {
    if height == 0 {
        return 0;
    }
    let halvings = (height - 1) / HALVING_INTERVAL;
    let halved = if halvings >= 63 {
        0
    } else {
        BLOCK_REWARD >> halvings
    };
    halved.max(TAIL_EMISSION)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TransactionType {
    Transfer,    // Transfer tokens
    Reward,      // Block production reward
    Certificate, // Anchor a certificate issuance
    Stake,       // Register / increase validator stake (on-chain)
    Withdraw,    // Reduce / exit validator stake (on-chain)
    Slash,       // Punish a proven equivocation (on-chain)
    Vest,        // Lock tokens for a recipient on a cliff + linear schedule
}

/// Upper bound on a vesting schedule's duration (~47 years at 15s blocks).
/// Bounds per-entry arithmetic and prevents nonsense schedules.
pub const MAX_VESTING_DURATION_BLOCKS: u64 = 100_000_000;

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
    /// sr25519 VRF public key (Stake transactions only): registered on-chain
    /// alongside the identity key for leader-election randomness.
    #[serde(default)]
    pub vrf_public_key: Option<String>,
    /// Credential registry action (Certificate transactions only).
    #[serde(default)]
    pub credential: Option<CredentialAction>,
    /// Vest transactions only: blocks after inclusion before ANY tokens
    /// release (the cliff), and total blocks over which the amount vests
    /// linearly. cliff <= duration.
    #[serde(default)]
    pub vesting_cliff_blocks: Option<u64>,
    #[serde(default)]
    pub vesting_duration_blocks: Option<u64>,
}

/// Issue or revoke an on-chain verifiable credential. Only the hash of the
/// credential document goes on-chain; the document stays private.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CredentialAction {
    pub id: String,
    pub subject: String,
    pub data_hash: String,
    #[serde(default)]
    pub revoke: bool,
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
            vrf_public_key: None,
            credential: None,
            vesting_cliff_blocks: None,
            vesting_duration_blocks: None,
        }
    }

    /// The block reward paid to the validator producing the block at
    /// `height` — the amount follows the deterministic halving schedule.
    pub fn new_reward(validator: &str, height: u64) -> Self {
        Self::new(
            None,
            validator.to_string(),
            block_reward(height),
            TransactionType::Reward,
        )
    }

    /// Canonical message a sender signs to authorize a transfer.
    pub fn transfer_signing_message(from: &str, to: &str, amount: u64, nonce: u64) -> String {
        format!("hikmalayer-transfer:{}:{}:{}:{}", from, to, amount, nonce)
    }

    /// Canonical message a validator signs to authorize a stake deposit.
    /// Binds the VRF public key so it cannot be substituted in transit.
    pub fn stake_signing_message(
        address: &str,
        amount: u64,
        nonce: u64,
        vrf_public_key: &str,
    ) -> String {
        format!(
            "hikmalayer-stake:{}:{}:{}:{}",
            address, amount, nonce, vrf_public_key
        )
    }

    /// Canonical message an issuer signs to issue or revoke a credential.
    pub fn credential_signing_message(action: &CredentialAction, nonce: u64) -> String {
        format!(
            "hikmalayer-credential:{}:{}:{}:{}:{}",
            action.id, action.subject, action.data_hash, action.revoke, nonce
        )
    }

    /// Canonical message a validator signs to authorize a stake withdrawal.
    pub fn withdraw_signing_message(address: &str, amount: u64, nonce: u64) -> String {
        format!("hikmalayer-withdraw:{}:{}:{}", address, amount, nonce)
    }

    /// Canonical message a sender signs to lock tokens into a vesting
    /// schedule for a recipient. Binds the full schedule so neither the
    /// cliff nor the duration can be altered in transit.
    pub fn vest_signing_message(
        from: &str,
        to: &str,
        amount: u64,
        cliff_blocks: u64,
        duration_blocks: u64,
        nonce: u64,
    ) -> String {
        format!(
            "hikmalayer-vest:{}:{}:{}:{}:{}:{}",
            from, to, amount, cliff_blocks, duration_blocks, nonce
        )
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
                let vrf_public_key = self
                    .vrf_public_key
                    .as_ref()
                    .ok_or_else(|| "Stake transaction missing VRF public key".to_string())?;
                let message =
                    Self::stake_signing_message(from, self.amount, self.nonce, vrf_public_key);
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
                // The exact amount follows the halving schedule for the
                // block's height; it is enforced in `Blockchain::validate_block_at`
                // where the height is known.
                Ok(())
            }
            TransactionType::Certificate => {
                if self.amount != 0 {
                    return Err("Certificate transaction must not carry value".to_string());
                }
                // Credential actions must be signed by the issuer; legacy
                // anchor transactions (no payload) need no signature.
                if let Some(action) = &self.credential {
                    let issuer = self
                        .from
                        .as_ref()
                        .ok_or_else(|| "Credential action missing issuer".to_string())?;
                    if action.id.trim().is_empty() || action.id.len() > 128 {
                        return Err("Credential id must be 1-128 characters".to_string());
                    }
                    if action.subject.len() > 256 || action.data_hash.len() > 128 {
                        return Err("Credential fields exceed size limits".to_string());
                    }
                    let message = Self::credential_signing_message(action, self.nonce);
                    self.verify_sender_signature(issuer, &message)?;
                }
                Ok(())
            }
            TransactionType::Vest => {
                let from = self
                    .from
                    .as_ref()
                    .ok_or_else(|| "Vest transaction missing sender".to_string())?;
                if self.amount == 0 {
                    return Err("Vest amount must be greater than zero".to_string());
                }
                if self.to.trim().is_empty() {
                    return Err("Vest transaction missing recipient".to_string());
                }
                let cliff = self
                    .vesting_cliff_blocks
                    .ok_or_else(|| "Vest transaction missing cliff".to_string())?;
                let duration = self
                    .vesting_duration_blocks
                    .ok_or_else(|| "Vest transaction missing duration".to_string())?;
                if duration == 0 || duration > MAX_VESTING_DURATION_BLOCKS {
                    return Err(format!(
                        "Vest duration must be 1..={} blocks",
                        MAX_VESTING_DURATION_BLOCKS
                    ));
                }
                if cliff > duration {
                    return Err("Vest cliff cannot exceed the duration".to_string());
                }
                let message =
                    Self::vest_signing_message(from, &self.to, self.amount, cliff, duration, self.nonce);
                self.verify_sender_signature(from, &message)
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
    fn reward_verification_enforces_recipient() {
        let reward = Transaction::new_reward("validator-1", 1);
        assert_eq!(reward.amount, BLOCK_REWARD);
        assert!(reward.verify_for_block("validator-1").is_ok());
        // Must pay the block validator.
        assert!(reward.verify_for_block("validator-2").is_err());
    }

    #[test]
    #[allow(clippy::assertions_on_constants)] // schedule sanity asserts are the point
    fn emission_halves_on_schedule_and_floors_at_the_tail() {
        assert_eq!(block_reward(0), 0); // genesis pays nothing
        assert_eq!(block_reward(1), BLOCK_REWARD);
        assert_eq!(block_reward(HALVING_INTERVAL), BLOCK_REWARD);
        assert_eq!(block_reward(HALVING_INTERVAL + 1), BLOCK_REWARD / 2);
        assert_eq!(block_reward(2 * HALVING_INTERVAL + 1), BLOCK_REWARD / 4);

        // The halvings floor at the tail emission and stay there forever —
        // the perpetual security budget. 5,000 >> 7 = 39 HKM < 50 HKM, so
        // epoch 8 (halvings = 7) is the first tail epoch.
        assert!(BLOCK_REWARD >> 7 < TAIL_EMISSION);
        assert!(BLOCK_REWARD >> 6 > TAIL_EMISSION);
        assert_eq!(block_reward(7 * HALVING_INTERVAL + 1), TAIL_EMISSION);
        assert_eq!(block_reward(100 * HALVING_INTERVAL + 1), TAIL_EMISSION);
        assert_eq!(block_reward(u64::MAX), TAIL_EMISSION);

        // Halving-phase emission + genesis lands just under the 100B cap:
        // sum over epochs of interval * reward, in whole HKM.
        let mut mined_hkm: u128 = 0;
        for epoch in 0..7u32 {
            let reward = (BLOCK_REWARD >> epoch).max(TAIL_EMISSION);
            mined_hkm += (HALVING_INTERVAL as u128) * (reward as u128)
                / (UNITS_PER_HKM as u128);
        }
        let genesis_hkm: u128 = 20_000_000_000;
        let total = genesis_hkm + mined_hkm;
        assert!(total > 99_000_000_000, "total at tail start: {total}");
        assert!(total <= 100_000_000_000, "total at tail start: {total}");
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
        let vrf_key = crate::consensus::vrf::derive_vrf_public_key(&private_key).unwrap();
        tx.vrf_public_key = Some(vrf_key.clone());
        let message = Transaction::stake_signing_message(&from, 100, 1, &vrf_key);
        tx.signature = Some(pos::sign_message(&message, &private_key).unwrap());
        assert!(tx.verify_for_block("validator-1").is_ok());

        // Wrong destination account: rejected.
        tx.to = "hkmsomewhere".to_string();
        assert!(tx.verify_for_block("validator-1").is_err());
    }
}
