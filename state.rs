//! The replicated on-chain state machine.
//!
//! Balances, the validator set, per-account nonces, and slashing records are
//! a deterministic function of the block history. Every block commits to the
//! resulting state via `state_root`, so any node can verify that any other
//! node executed the chain correctly — no node-local balance bookkeeping.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

use crate::blockchain::transaction::{Transaction, TransactionType};
use crate::consensus::pos::{self, Staker};

/// Internal account holding all staked funds.
pub const STAKING_POOL_ACCOUNT: &str = "__staking_pool__";

/// Consensus constant: percentage of stake burned for a proven equivocation.
pub const SLASH_PERCENT: u64 = 10;

/// Stake registered at genesis for the genesis validator (the treasury).
pub const GENESIS_VALIDATOR_STAKE: u64 = 1_000;

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct StakeInfo {
    pub stake: u64,
    pub public_key: String,
    /// sr25519 VRF public key used for unbiasable leader-election
    /// randomness. Registered on-chain with the stake.
    #[serde(default)]
    pub vrf_public_key: String,
}

/// An on-chain verifiable credential: the issuer anchors a hash of the
/// credential document (the document itself stays private/off-chain) bound
/// to a subject. Revocation is a first-class on-chain operation by the
/// issuer. Any third party can verify a credential against the state root.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct CredentialRecord {
    pub issuer: String,
    pub subject: String,
    pub data_hash: String,
    pub issued_at: String,
    pub revoked: bool,
}

/// Deterministic chain state. All maps are `BTreeMap` so serialization —
/// and therefore the state root — is canonical.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ChainState {
    pub balances: BTreeMap<String, u64>,
    pub stakers: BTreeMap<String, StakeInfo>,
    pub nonces: BTreeMap<String, u64>,
    /// On-chain verifiable credentials, keyed by credential ID.
    #[serde(default)]
    pub credentials: BTreeMap<String, CredentialRecord>,
    /// "{validator}:{height}" offenses already punished (prevents double
    /// slashing from the same equivocation proof).
    pub slashed_offenses: BTreeMap<String, u64>,
    pub total_supply: u64,
    pub burned: u64,
}

impl ChainState {
    /// State at genesis: the entire initial supply is allocated to the
    /// treasury, and (when its keys are known) the treasury is registered as
    /// the genesis validator so the chain can bootstrap block production.
    pub fn genesis(
        treasury_address: &str,
        treasury_public_key: Option<&str>,
        treasury_vrf_public_key: Option<&str>,
        initial_supply: u64,
    ) -> Self {
        let mut state = ChainState {
            total_supply: initial_supply,
            ..Default::default()
        };
        state
            .balances
            .insert(treasury_address.to_string(), initial_supply);

        if let Some(public_key) = treasury_public_key {
            let stake = GENESIS_VALIDATOR_STAKE.min(initial_supply);
            let treasury_balance = state.balances.get_mut(treasury_address).unwrap();
            *treasury_balance -= stake;
            *state
                .balances
                .entry(STAKING_POOL_ACCOUNT.to_string())
                .or_insert(0) += stake;
            state.stakers.insert(
                treasury_address.to_string(),
                StakeInfo {
                    stake,
                    public_key: public_key.to_string(),
                    vrf_public_key: treasury_vrf_public_key.unwrap_or_default().to_string(),
                },
            );
        }

        state
    }

    /// Canonical commitment to the full state.
    pub fn state_root(&self) -> String {
        let canonical =
            serde_json::to_string(self).expect("chain state serialization cannot fail");
        format!("{:x}", Sha256::digest(canonical.as_bytes()))
    }

    pub fn balance_of(&self, account: &str) -> u64 {
        self.balances.get(account).copied().unwrap_or(0)
    }

    pub fn nonce_of(&self, account: &str) -> u64 {
        self.nonces.get(account).copied().unwrap_or(0)
    }

    /// The current validator set, deterministically ordered by address.
    pub fn validator_set(&self) -> Vec<Staker> {
        self.stakers
            .iter()
            .filter(|(_, info)| info.stake > 0)
            .map(|(address, info)| Staker {
                address: address.clone(),
                stake: info.stake,
                public_key: Some(info.public_key.clone()),
            })
            .collect()
    }

    fn consume_nonce(&mut self, account: &str, nonce: u64) -> Result<(), String> {
        let expected = self.nonce_of(account) + 1;
        if nonce != expected {
            return Err(format!(
                "Invalid nonce for {}: expected {}, got {}",
                account, expected, nonce
            ));
        }
        self.nonces.insert(account.to_string(), nonce);
        Ok(())
    }

    fn debit(&mut self, account: &str, amount: u64) -> Result<(), String> {
        let balance = self.balance_of(account);
        if balance < amount {
            return Err(format!(
                "Insufficient balance for {}: has {}, needs {}",
                account, balance, amount
            ));
        }
        self.balances.insert(account.to_string(), balance - amount);
        Ok(())
    }

    fn credit(&mut self, account: &str, amount: u64) {
        *self.balances.entry(account.to_string()).or_insert(0) += amount;
    }

    /// Apply one transaction. Stateless validity (signature schemes, reward
    /// shape) is checked by `Transaction::verify_for_block`; this method
    /// enforces the stateful rules: nonces, balances, stake accounting,
    /// registered-key checks, and slashing.
    pub fn apply_transaction(&mut self, tx: &Transaction) -> Result<(), String> {
        match tx.transaction_type {
            TransactionType::Transfer => {
                let from = tx
                    .from
                    .as_ref()
                    .ok_or_else(|| "Transfer missing sender".to_string())?;
                self.consume_nonce(from, tx.nonce)?;
                self.debit(from, tx.amount)?;
                self.credit(&tx.to, tx.amount);
                Ok(())
            }
            TransactionType::Stake => {
                let from = tx
                    .from
                    .as_ref()
                    .ok_or_else(|| "Stake missing sender".to_string())?;
                let public_key = tx
                    .public_key
                    .as_ref()
                    .ok_or_else(|| "Stake missing public key".to_string())?;
                let vrf_public_key = tx
                    .vrf_public_key
                    .as_ref()
                    .ok_or_else(|| "Stake missing VRF public key".to_string())?;
                self.consume_nonce(from, tx.nonce)?;
                self.debit(from, tx.amount)?;
                self.credit(STAKING_POOL_ACCOUNT, tx.amount);
                let entry = self.stakers.entry(from.clone()).or_default();
                entry.stake += tx.amount;
                entry.public_key = public_key.clone();
                entry.vrf_public_key = vrf_public_key.clone();
                Ok(())
            }
            TransactionType::Withdraw => {
                let from = tx
                    .from
                    .as_ref()
                    .ok_or_else(|| "Withdraw missing sender".to_string())?;
                let signature = tx
                    .signature
                    .as_ref()
                    .ok_or_else(|| "Withdraw missing signature".to_string())?;

                // Withdrawals are authorized by the validator's key as
                // registered ON CHAIN — a stateful check by nature.
                let info = self
                    .stakers
                    .get(from)
                    .ok_or_else(|| format!("No stake registered for {}", from))?
                    .clone();
                let message = Transaction::withdraw_signing_message(from, tx.amount, tx.nonce);
                if !pos::verify_message(&message, &info.public_key, signature) {
                    return Err("Withdraw signature does not match registered key".to_string());
                }
                if info.stake < tx.amount {
                    return Err(format!(
                        "Insufficient stake for {}: has {}, needs {}",
                        from, info.stake, tx.amount
                    ));
                }

                self.consume_nonce(from, tx.nonce)?;
                self.debit(STAKING_POOL_ACCOUNT, tx.amount)?;
                self.credit(from, tx.amount);
                let remaining = info.stake - tx.amount;
                if remaining == 0 {
                    self.stakers.remove(from);
                } else {
                    self.stakers.get_mut(from).unwrap().stake = remaining;
                }
                Ok(())
            }
            TransactionType::Reward => {
                self.credit(&tx.to, tx.amount);
                self.total_supply += tx.amount;
                Ok(())
            }
            TransactionType::Certificate => {
                // Legacy anchor transactions (no credential payload) remain
                // valid no-ops; credential actions mutate the registry.
                let Some(action) = &tx.credential else {
                    return Ok(());
                };
                let issuer = tx
                    .from
                    .as_ref()
                    .ok_or_else(|| "Credential action missing issuer".to_string())?;
                self.consume_nonce(issuer, tx.nonce)?;

                if action.revoke {
                    let record = self
                        .credentials
                        .get_mut(&action.id)
                        .ok_or_else(|| format!("Credential {} not found", action.id))?;
                    if record.issuer != *issuer {
                        return Err("Only the issuer can revoke a credential".to_string());
                    }
                    record.revoked = true;
                } else {
                    if self.credentials.contains_key(&action.id) {
                        return Err(format!("Credential {} already exists", action.id));
                    }
                    self.credentials.insert(
                        action.id.clone(),
                        CredentialRecord {
                            issuer: issuer.clone(),
                            subject: action.subject.clone(),
                            data_hash: action.data_hash.clone(),
                            issued_at: tx.timestamp.to_rfc3339(),
                            revoked: false,
                        },
                    );
                }
                Ok(())
            }
            TransactionType::Slash => {
                let proof = tx
                    .slash_proof
                    .as_ref()
                    .ok_or_else(|| "Slash missing proof".to_string())?;
                let validator = &tx.to;

                // The offending key must be the validator's registered key.
                let info = self
                    .stakers
                    .get(validator)
                    .ok_or_else(|| format!("No stake registered for {}", validator))?
                    .clone();
                let proof_key = proof
                    .block_a
                    .validator_public_key
                    .as_deref()
                    .unwrap_or_default();
                if proof_key != info.public_key {
                    return Err("Slash proof key does not match registered key".to_string());
                }

                let offense_key = format!("{}:{}", validator, proof.block_a.index);
                if self.slashed_offenses.contains_key(&offense_key) {
                    return Err("Offense already slashed".to_string());
                }

                let slashed = info.stake.saturating_mul(SLASH_PERCENT) / 100;
                if slashed == 0 {
                    return Err("Nothing to slash".to_string());
                }
                self.debit(STAKING_POOL_ACCOUNT, slashed)?;
                self.burned += slashed;
                self.total_supply = self.total_supply.saturating_sub(slashed);
                let remaining = info.stake - slashed;
                if remaining == 0 {
                    self.stakers.remove(validator);
                } else {
                    self.stakers.get_mut(validator).unwrap().stake = remaining;
                }
                self.slashed_offenses.insert(offense_key, slashed);
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blockchain::transaction::BLOCK_REWARD;

    fn wallet(seed: u8) -> (String, String, String) {
        let private_key = hex::encode([seed; 32]);
        let public_key = pos::derive_public_key(&private_key).unwrap();
        let address = pos::derive_address(&public_key).unwrap();
        (address, public_key, private_key)
    }

    fn genesis_state() -> (ChainState, String, String, String) {
        let (address, public_key, private_key) = wallet(1);
        let vrf_key = crate::consensus::vrf::derive_vrf_public_key(&private_key).unwrap();
        let state = ChainState::genesis(&address, Some(&public_key), Some(&vrf_key), 1_000_000);
        (state, address, public_key, private_key)
    }

    #[test]
    fn genesis_allocates_supply_and_registers_validator() {
        let (state, treasury, _, _) = genesis_state();
        assert_eq!(
            state.balance_of(&treasury),
            1_000_000 - GENESIS_VALIDATOR_STAKE
        );
        assert_eq!(
            state.balance_of(STAKING_POOL_ACCOUNT),
            GENESIS_VALIDATOR_STAKE
        );
        assert_eq!(state.validator_set().len(), 1);
        assert_eq!(state.total_supply, 1_000_000);
    }

    #[test]
    fn state_root_is_deterministic_and_sensitive() {
        let (state_a, ..) = genesis_state();
        let (mut state_b, ..) = genesis_state();
        assert_eq!(state_a.state_root(), state_b.state_root());
        state_b.credit("hkmsomeone", 1);
        assert_ne!(state_a.state_root(), state_b.state_root());
    }

    #[test]
    fn transfer_updates_balances_and_nonce() {
        let (mut state, treasury, _, _) = genesis_state();
        let mut tx = Transaction::new(
            Some(treasury.clone()),
            "hkmrecipient".to_string(),
            100,
            TransactionType::Transfer,
        );
        tx.nonce = 1;
        state.apply_transaction(&tx).unwrap();
        assert_eq!(state.balance_of("hkmrecipient"), 100);
        assert_eq!(state.nonce_of(&treasury), 1);

        // Same nonce cannot apply twice.
        assert!(state.apply_transaction(&tx).is_err());
    }

    #[test]
    fn transfer_rejects_overdraft() {
        let (mut state, ..) = genesis_state();
        let (poor, ..) = wallet(9);
        let mut tx = Transaction::new(
            Some(poor),
            "hkmrecipient".to_string(),
            1,
            TransactionType::Transfer,
        );
        tx.nonce = 1;
        assert!(state.apply_transaction(&tx).is_err());
    }

    #[test]
    fn stake_and_withdraw_roundtrip() {
        let (mut state, treasury, _, treasury_key) = genesis_state();
        let (address, public_key, private_key) = wallet(2);

        // Fund the new validator from treasury.
        let mut fund = Transaction::new(
            Some(treasury.clone()),
            address.clone(),
            500,
            TransactionType::Transfer,
        );
        fund.nonce = 1;
        state.apply_transaction(&fund).unwrap();
        let _ = treasury_key;

        // Stake.
        let mut stake = Transaction::new(
            Some(address.clone()),
            STAKING_POOL_ACCOUNT.to_string(),
            300,
            TransactionType::Stake,
        );
        stake.nonce = 1;
        stake.public_key = Some(public_key.clone());
        stake.vrf_public_key =
            Some(crate::consensus::vrf::derive_vrf_public_key(&private_key).unwrap());
        state.apply_transaction(&stake).unwrap();
        assert_eq!(state.validator_set().len(), 2);
        assert_eq!(state.balance_of(&address), 200);

        // Withdraw with a valid signature over the withdraw message.
        let mut withdraw = Transaction::new(
            Some(address.clone()),
            address.clone(),
            300,
            TransactionType::Withdraw,
        );
        withdraw.nonce = 2;
        let message = Transaction::withdraw_signing_message(&address, 300, 2);
        withdraw.signature = Some(pos::sign_message(&message, &private_key).unwrap());
        state.apply_transaction(&withdraw).unwrap();
        assert_eq!(state.balance_of(&address), 500);
        assert_eq!(state.validator_set().len(), 1);
    }

    #[test]
    fn withdraw_rejects_wrong_key() {
        let (mut state, treasury, _, _) = genesis_state();
        let (_, _, intruder_key) = wallet(7);
        let mut withdraw = Transaction::new(
            Some(treasury.clone()),
            treasury.clone(),
            10,
            TransactionType::Withdraw,
        );
        withdraw.nonce = 1;
        let message = Transaction::withdraw_signing_message(&treasury, 10, 1);
        withdraw.signature = Some(pos::sign_message(&message, &intruder_key).unwrap());
        assert!(state.apply_transaction(&withdraw).is_err());
    }

    #[test]
    fn reward_mints_supply() {
        let (mut state, treasury, _, _) = genesis_state();
        let reward = Transaction::new_reward(&treasury);
        let supply_before = state.total_supply;
        state.apply_transaction(&reward).unwrap();
        assert_eq!(state.total_supply, supply_before + BLOCK_REWARD);
    }
}
