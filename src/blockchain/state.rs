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

/// Flat protocol fee charged on value-bearing transactions (Transfer,
/// Stake, Withdraw). Credited to the block validator. Credential actions
/// stay free (anti-spam via nonces and mempool caps).
pub const TX_FEE: u64 = 1;

/// Blocks a withdrawn stake stays locked (and slashable) before release.
pub const UNBONDING_BLOCKS: u64 = 20;

/// How far back (in blocks) an equivocation proof is accepted. Equal to the
/// unbonding period so misbehaving stake can never exit before a slash.
pub const SLASHING_WINDOW_BLOCKS: u64 = 20;

/// Stake in the process of unbonding: still in the pool, still slashable,
/// released to the owner's balance at `release_height`.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct UnbondingEntry {
    pub amount: u64,
    pub release_height: u64,
}

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
    /// Stake awaiting release after withdrawal (still slashable).
    #[serde(default)]
    pub unbonding: BTreeMap<String, Vec<UnbondingEntry>>,
    /// Fees collected within the current block; paid to the validator and
    /// zeroed by `end_block`, so it is always 0 at block boundaries.
    #[serde(default)]
    pub fee_pot: u64,
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

    /// Total stake currently unbonding for an account.
    pub fn unbonding_total(&self, account: &str) -> u64 {
        self.unbonding
            .get(account)
            .map(|entries| entries.iter().map(|e| e.amount).sum())
            .unwrap_or(0)
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

    /// Apply one transaction at `height`. Stateless validity (signature
    /// schemes, reward shape) is checked by `Transaction::verify_for_block`;
    /// this method enforces the stateful rules: nonces, balances + fees,
    /// stake accounting with unbonding, registered-key checks, and slashing.
    pub fn apply_transaction(&mut self, tx: &Transaction, height: u64) -> Result<(), String> {
        match tx.transaction_type {
            TransactionType::Transfer => {
                let from = tx
                    .from
                    .as_ref()
                    .ok_or_else(|| "Transfer missing sender".to_string())?;
                self.consume_nonce(from, tx.nonce)?;
                self.debit(from, tx.amount + TX_FEE)?;
                self.credit(&tx.to, tx.amount);
                self.fee_pot += TX_FEE;
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
                self.debit(from, tx.amount + TX_FEE)?;
                self.credit(STAKING_POOL_ACCOUNT, tx.amount);
                self.fee_pot += TX_FEE;
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
                // The withdrawal fee comes from liquid balance; the stake
                // itself enters unbonding — it stays in the pool, remains
                // slashable, and is released after UNBONDING_BLOCKS.
                self.debit(from, TX_FEE)?;
                self.fee_pot += TX_FEE;
                self.stakers.get_mut(from).unwrap().stake = info.stake - tx.amount;
                self.unbonding
                    .entry(from.clone())
                    .or_default()
                    .push(UnbondingEntry {
                        amount: tx.amount,
                        release_height: height + UNBONDING_BLOCKS,
                    });
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

                // Slashing window: proofs must land while the offending
                // stake is still bonded or unbonding.
                if proof.block_a.index + SLASHING_WINDOW_BLOCKS < height {
                    return Err("Equivocation proof outside the slashing window".to_string());
                }

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

                // Unbonding stake is still slashable: base = bonded + unbonding.
                let base = info.stake + self.unbonding_total(validator);
                let slashed = base.saturating_mul(SLASH_PERCENT) / 100;
                if slashed == 0 {
                    return Err("Nothing to slash".to_string());
                }
                self.debit(STAKING_POOL_ACCOUNT, slashed)?;
                self.burned += slashed;
                self.total_supply = self.total_supply.saturating_sub(slashed);

                // Deduct from bonded stake first, then oldest unbonding.
                let mut remaining = slashed;
                let take_bonded = remaining.min(info.stake);
                self.stakers.get_mut(validator).unwrap().stake = info.stake - take_bonded;
                remaining -= take_bonded;
                if remaining > 0 {
                    if let Some(entries) = self.unbonding.get_mut(validator) {
                        for entry in entries.iter_mut() {
                            let take = remaining.min(entry.amount);
                            entry.amount -= take;
                            remaining -= take;
                            if remaining == 0 {
                                break;
                            }
                        }
                        entries.retain(|e| e.amount > 0);
                        if entries.is_empty() {
                            self.unbonding.remove(validator);
                        }
                    }
                }

                self.slashed_offenses.insert(offense_key, slashed);
                Ok(())
            }
        }
    }
}

impl ChainState {
    /// Block-boundary housekeeping, applied identically by every node after
    /// the block's transactions: release matured unbonding stake and pay the
    /// block's collected fees to its validator.
    pub fn end_block(&mut self, height: u64, validator: &str) {
        // Release matured unbonding entries (pool → owner balance).
        let accounts: Vec<String> = self.unbonding.keys().cloned().collect();
        for account in accounts {
            let mut released = 0u64;
            if let Some(entries) = self.unbonding.get_mut(&account) {
                entries.retain(|entry| {
                    if entry.release_height <= height {
                        released += entry.amount;
                        false
                    } else {
                        true
                    }
                });
                if entries.is_empty() {
                    self.unbonding.remove(&account);
                }
            }
            if released > 0 {
                let _ = self.debit(STAKING_POOL_ACCOUNT, released);
                self.credit(&account, released);
            }
            // Fully exited validators leave the staker set once nothing
            // remains bonded or unbonding.
            if self.stakers.get(&account).is_some_and(|i| i.stake == 0)
                && !self.unbonding.contains_key(&account)
            {
                self.stakers.remove(&account);
            }
        }

        // Pay this block's fees to its validator.
        if self.fee_pot > 0 {
            let fees = self.fee_pot;
            self.fee_pot = 0;
            self.credit(validator, fees);
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
        state.apply_transaction(&tx, 1).unwrap();
        assert_eq!(state.balance_of("hkmrecipient"), 100);
        assert_eq!(state.nonce_of(&treasury), 1);

        // Same nonce cannot apply twice.
        assert!(state.apply_transaction(&tx, 1).is_err());
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
        assert!(state.apply_transaction(&tx, 1).is_err());
    }

    #[test]
    fn stake_withdraw_unbonding_lifecycle() {
        let (mut state, treasury, _, _) = genesis_state();
        let (address, public_key, private_key) = wallet(2);

        // Fund the new validator from treasury (sender pays the fee).
        let mut fund = Transaction::new(
            Some(treasury.clone()),
            address.clone(),
            500,
            TransactionType::Transfer,
        );
        fund.nonce = 1;
        state.apply_transaction(&fund, 1).unwrap();

        // Stake 300 (+1 fee).
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
        state.apply_transaction(&stake, 1).unwrap();
        assert_eq!(state.validator_set().len(), 2);
        assert_eq!(state.balance_of(&address), 500 - 300 - TX_FEE);

        // Withdraw 300 (+1 fee) at height 2: funds enter UNBONDING, they
        // are NOT immediately spendable.
        let mut withdraw = Transaction::new(
            Some(address.clone()),
            address.clone(),
            300,
            TransactionType::Withdraw,
        );
        withdraw.nonce = 2;
        let message = Transaction::withdraw_signing_message(&address, 300, 2);
        withdraw.signature = Some(pos::sign_message(&message, &private_key).unwrap());
        state.apply_transaction(&withdraw, 2).unwrap();

        assert_eq!(state.balance_of(&address), 500 - 300 - 2 * TX_FEE);
        assert_eq!(state.unbonding_total(&address), 300);
        // Exited from the active set, but keys retained while unbonding.
        assert_eq!(state.validator_set().len(), 1);
        assert!(state.stakers.contains_key(&address));

        // Not released before maturity.
        state.end_block(2 + UNBONDING_BLOCKS - 1, &treasury);
        assert_eq!(state.unbonding_total(&address), 300);

        // Released at maturity; the fully exited validator entry is gone.
        state.end_block(2 + UNBONDING_BLOCKS, &treasury);
        assert_eq!(state.unbonding_total(&address), 0);
        assert_eq!(state.balance_of(&address), 500 - 2 * TX_FEE);
        assert!(!state.stakers.contains_key(&address));
    }

    #[test]
    fn slash_reaches_unbonding_stake_and_respects_window() {
        let (mut state, treasury, treasury_pub, treasury_key) = genesis_state();
        let _ = treasury_pub;

        // Treasury withdraws 400 of its 1000 genesis stake at height 5.
        let mut withdraw = Transaction::new(
            Some(treasury.clone()),
            treasury.clone(),
            400,
            TransactionType::Withdraw,
        );
        withdraw.nonce = 1;
        let message = Transaction::withdraw_signing_message(&treasury, 400, 1);
        withdraw.signature = Some(pos::sign_message(&message, &treasury_key).unwrap());
        state.apply_transaction(&withdraw, 5).unwrap();
        assert_eq!(state.unbonding_total(&treasury), 400);

        // Build a slash tx (proof internals are validated statelessly by
        // verify_for_block; apply checks the stateful parts we exercise by
        // constructing the proof through the chain tests — here we check the
        // window + base math using a minimal proof object).
        use crate::blockchain::block::Block;
        use crate::blockchain::transaction::SlashProof;
        let make_block = |memo: &str| {
            Block::new(
                7,
                vec![memo.to_string()],
                "prev".to_string(),
                2,
                Some(treasury.clone()),
                Some(state.stakers[&treasury].public_key.clone()),
                None,
                "root".to_string(),
            )
        };
        let mut slash = Transaction::new(None, treasury.clone(), 0, TransactionType::Slash);
        slash.slash_proof = Some(SlashProof {
            block_a: make_block("a"),
            block_b: make_block("b"),
        });

        // Outside the window: rejected.
        let err = state
            .apply_transaction(&slash, 7 + SLASHING_WINDOW_BLOCKS + 1)
            .unwrap_err();
        assert!(err.contains("window"), "{err}");

        // Inside the window: slashes 10% of bonded (600) + unbonding (400).
        let pool_before = state.balance_of(STAKING_POOL_ACCOUNT);
        state.apply_transaction(&slash, 8).unwrap();
        assert_eq!(state.burned, 100);
        assert_eq!(state.balance_of(STAKING_POOL_ACCOUNT), pool_before - 100);
        // Bonded stake absorbs the deduction first.
        assert_eq!(state.stakers[&treasury].stake, 500);
        assert_eq!(state.unbonding_total(&treasury), 400);
    }

    #[test]
    fn fees_flow_to_the_block_validator() {
        let (mut state, treasury, _, _) = genesis_state();
        let mut tx = Transaction::new(
            Some(treasury.clone()),
            "hkmrecipient".to_string(),
            100,
            TransactionType::Transfer,
        );
        tx.nonce = 1;
        state.apply_transaction(&tx, 1).unwrap();
        assert_eq!(state.fee_pot, TX_FEE);

        state.end_block(1, "hkmvalidator");
        assert_eq!(state.fee_pot, 0);
        assert_eq!(state.balance_of("hkmvalidator"), TX_FEE);
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
        assert!(state.apply_transaction(&withdraw, 1).is_err());
    }

    #[test]
    fn reward_mints_supply() {
        let (mut state, treasury, _, _) = genesis_state();
        let reward = Transaction::new_reward(&treasury);
        let supply_before = state.total_supply;
        state.apply_transaction(&reward, 1).unwrap();
        assert_eq!(state.total_supply, supply_before + BLOCK_REWARD);
    }
}
