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

use crate::blockchain::transaction::UNITS_PER_HKM;

/// Internal account holding all staked funds.
pub const STAKING_POOL_ACCOUNT: &str = "__staking_pool__";

/// Internal account holding all unvested (locked) funds.
pub const VESTING_POOL_ACCOUNT: &str = "__vesting_pool__";

/// Consensus constant: percentage of stake burned for a proven equivocation.
pub const SLASH_PERCENT: u64 = 10;

/// Stake registered at genesis for the genesis validator (the treasury):
/// 1,000,000 HKM.
pub const GENESIS_VALIDATOR_STAKE: u64 = 1_000_000 * UNITS_PER_HKM;

/// Minimum total stake to be (or remain) a validator: 10,000 HKM. A Stake
/// transaction must leave the validator at or above this floor, and a
/// Withdraw must leave either zero (full exit via unbonding) or at least
/// the floor — preventing trivial-stake spam validators from bloating the
/// leader-election set. (Slashing may push a validator below the floor;
/// it keeps producing until it exits or tops back up.)
pub const MIN_VALIDATOR_STAKE: u64 = 10_000 * UNITS_PER_HKM;

/// Minimum (and genesis) base fee charged on value-bearing transactions
/// (Transfer, Stake, Withdraw, Vest): 0.001 HKM. Credited to the block
/// validator. Credential actions stay free (anti-spam via nonces and
/// mempool caps). The effective fee is the dynamic `ChainState::base_fee`,
/// which floors at this value.
pub const TX_FEE: u64 = UNITS_PER_HKM / 1_000;

/// Congestion target for the fee market: when a block carries more than this
/// many fee-paying transactions the base fee rises; fewer and it falls.
pub const BASE_FEE_TARGET_TXS: u64 = 50;

/// Upper bound on the base fee so it cannot run away: 100 HKM.
pub const BASE_FEE_MAX: u64 = 100 * UNITS_PER_HKM;

/// Deterministic EIP-1559-style base-fee update: at most a 1/8 (12.5%) step
/// per block toward relieving or applying congestion, bounded to
/// `[TX_FEE, BASE_FEE_MAX]`. Because it is a pure function of the parent
/// block's fee-paying tx count, every node computes the identical next fee.
pub fn next_base_fee(current: u64, fee_paying_txs: u64) -> u64 {
    let target = BASE_FEE_TARGET_TXS;
    if fee_paying_txs == target {
        return current.clamp(TX_FEE, BASE_FEE_MAX);
    }
    let max_step = (current / 8).max(1);
    let next = if fee_paying_txs > target {
        let step = (max_step.saturating_mul(fee_paying_txs - target) / target).max(1);
        current.saturating_add(step)
    } else {
        let step = (max_step.saturating_mul(target - fee_paying_txs) / target).max(1);
        current.saturating_sub(step)
    };
    next.clamp(TX_FEE, BASE_FEE_MAX)
}

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

/// Tokens locked for a recipient on a cliff + linear schedule. Nothing
/// releases before `cliff_height`; from there the amount accrued linearly
/// since `start_height` releases block by block until `end_height`, when
/// the full total has been paid out. Funds sit in the vesting pool until
/// released, so supply accounting stays exact.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct VestingEntry {
    pub total: u64,
    pub released: u64,
    pub start_height: u64,
    pub cliff_height: u64,
    pub end_height: u64,
}

impl VestingEntry {
    /// Amount vested (cumulative) at `height`. Linear between start and
    /// end, gated by the cliff; u128 intermediate so `total * elapsed`
    /// cannot overflow for any legal schedule.
    pub fn vested_at(&self, height: u64) -> u64 {
        if height < self.cliff_height {
            return 0;
        }
        if height >= self.end_height {
            return self.total;
        }
        let elapsed = (height - self.start_height) as u128;
        let span = (self.end_height - self.start_height) as u128;
        ((self.total as u128 * elapsed) / span) as u64
    }
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
    /// Tokens vesting toward each recipient (team/investor lockups).
    #[serde(default)]
    pub vesting: BTreeMap<String, Vec<VestingEntry>>,
    /// Genesis-configured validator allowlist. When NON-EMPTY, only listed
    /// addresses may register a NEW stake (existing validators may top up);
    /// empty means permissionless staking. Set once at genesis and part of
    /// the state root, so every node enforces the identical policy — this
    /// is the honest "permissioned hybrid at launch" lever, opened later
    /// via a scheduled network upgrade.
    #[serde(default)]
    pub validator_allowlist: std::collections::BTreeSet<String>,
    /// Fees collected within the current block; paid to the validator and
    /// zeroed by `end_block`, so it is always 0 at block boundaries.
    #[serde(default)]
    pub fee_pot: u64,
    /// Current dynamic base fee (per value-bearing transaction). Updated
    /// deterministically each block from the parent's congestion.
    #[serde(default = "default_base_fee")]
    pub base_fee: u64,
    pub total_supply: u64,
    pub burned: u64,
}

fn default_base_fee() -> u64 {
    TX_FEE
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
        validator_allowlist: &[String],
    ) -> Self {
        let mut state = ChainState {
            total_supply: initial_supply,
            base_fee: TX_FEE,
            validator_allowlist: validator_allowlist.iter().cloned().collect(),
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
                let fee = self.base_fee;
                self.debit(from, tx.amount + fee)?;
                self.credit(&tx.to, tx.amount);
                self.fee_pot += fee;
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
                // Launch posture: when an allowlist is configured, only
                // listed addresses may JOIN the validator set (existing
                // validators may add stake). Checked before any mutation.
                if !self.validator_allowlist.is_empty()
                    && !self.stakers.contains_key(from)
                    && !self.validator_allowlist.contains(from)
                {
                    return Err(format!(
                        "Validator registration is allowlist-gated at this network's genesis; \
                         {} is not on the allowlist",
                        from
                    ));
                }
                // Validator floor: the resulting total stake must meet the
                // minimum (checked before any state mutation).
                let current = self.stakers.get(from).map(|i| i.stake).unwrap_or(0);
                let resulting = current.saturating_add(tx.amount);
                if resulting < MIN_VALIDATOR_STAKE {
                    return Err(format!(
                        "Stake below the validator minimum: {} would hold {}, need {}",
                        from, resulting, MIN_VALIDATOR_STAKE
                    ));
                }
                self.consume_nonce(from, tx.nonce)?;
                let fee = self.base_fee;
                self.debit(from, tx.amount + fee)?;
                self.credit(STAKING_POOL_ACCOUNT, tx.amount);
                self.fee_pot += fee;
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
                // Validator floor: a withdrawal must either exit fully
                // (remaining stake 0, released through unbonding) or leave
                // at least the minimum bonded.
                let remaining = info.stake - tx.amount;
                if remaining != 0 && remaining < MIN_VALIDATOR_STAKE {
                    return Err(format!(
                        "Withdrawal would leave {} below the validator minimum ({}); \
                         withdraw the full stake to exit",
                        remaining, MIN_VALIDATOR_STAKE
                    ));
                }

                self.consume_nonce(from, tx.nonce)?;
                // The withdrawal fee comes from liquid balance; the stake
                // itself enters unbonding — it stays in the pool, remains
                // slashable, and is released after UNBONDING_BLOCKS.
                let fee = self.base_fee;
                self.debit(from, fee)?;
                self.fee_pot += fee;
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
            TransactionType::Vest => {
                let from = tx
                    .from
                    .as_ref()
                    .ok_or_else(|| "Vest missing sender".to_string())?;
                let cliff = tx
                    .vesting_cliff_blocks
                    .ok_or_else(|| "Vest missing cliff".to_string())?;
                let duration = tx
                    .vesting_duration_blocks
                    .ok_or_else(|| "Vest missing duration".to_string())?;
                if duration == 0 || cliff > duration {
                    return Err("Invalid vesting schedule".to_string());
                }
                self.consume_nonce(from, tx.nonce)?;
                let fee = self.base_fee;
                self.debit(from, tx.amount + fee)?;
                self.credit(VESTING_POOL_ACCOUNT, tx.amount);
                self.fee_pot += fee;
                self.vesting
                    .entry(tx.to.clone())
                    .or_default()
                    .push(VestingEntry {
                        total: tx.amount,
                        released: 0,
                        start_height: height,
                        cliff_height: height + cliff,
                        end_height: height + duration,
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

        // Release newly vested tokens (pool → recipient balance). Linear
        // accrual gated by each entry's cliff; deterministic because it is
        // a pure function of (entry, height). Completed entries drop out.
        let recipients: Vec<String> = self.vesting.keys().cloned().collect();
        for recipient in recipients {
            let mut newly_released = 0u64;
            if let Some(entries) = self.vesting.get_mut(&recipient) {
                for entry in entries.iter_mut() {
                    let vested = entry.vested_at(height);
                    if vested > entry.released {
                        newly_released += vested - entry.released;
                        entry.released = vested;
                    }
                }
                entries.retain(|entry| entry.released < entry.total);
                if entries.is_empty() {
                    self.vesting.remove(&recipient);
                }
            }
            if newly_released > 0 {
                let _ = self.debit(VESTING_POOL_ACCOUNT, newly_released);
                self.credit(&recipient, newly_released);
            }
        }

        // Fee market: derive the number of fee-paying transactions in this
        // block from the pot (all paid the same base fee), then set the base
        // fee for the NEXT block. Deterministic — every node computes it.
        let fee_paying_txs = if self.base_fee > 0 {
            self.fee_pot / self.base_fee
        } else {
            0
        };
        self.base_fee = next_base_fee(self.base_fee, fee_paying_txs);

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

    /// Test genesis supply: 100M HKM — comfortably above the genesis
    /// validator stake and every amount the tests move around.
    const TEST_SUPPLY: u64 = 100_000_000 * UNITS_PER_HKM;

    fn wallet(seed: u8) -> (String, String, String) {
        let private_key = hex::encode([seed; 32]);
        let public_key = pos::derive_public_key(&private_key).unwrap();
        let address = pos::derive_address(&public_key).unwrap();
        (address, public_key, private_key)
    }

    fn genesis_state() -> (ChainState, String, String, String) {
        let (address, public_key, private_key) = wallet(1);
        let vrf_key = crate::consensus::vrf::derive_vrf_public_key(&private_key).unwrap();
        let state = ChainState::genesis(&address, Some(&public_key), Some(&vrf_key), TEST_SUPPLY, &[]);
        (state, address, public_key, private_key)
    }

    #[test]
    fn genesis_allocates_supply_and_registers_validator() {
        let (state, treasury, _, _) = genesis_state();
        assert_eq!(
            state.balance_of(&treasury),
            TEST_SUPPLY - GENESIS_VALIDATOR_STAKE
        );
        assert_eq!(
            state.balance_of(STAKING_POOL_ACCOUNT),
            GENESIS_VALIDATOR_STAKE
        );
        assert_eq!(state.validator_set().len(), 1);
        assert_eq!(state.total_supply, TEST_SUPPLY);
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

        let stake_amount = MIN_VALIDATOR_STAKE;
        let funded = stake_amount + 10 * TX_FEE;

        // Fund the new validator from treasury (sender pays the fee).
        let mut fund = Transaction::new(
            Some(treasury.clone()),
            address.clone(),
            funded,
            TransactionType::Transfer,
        );
        fund.nonce = 1;
        state.apply_transaction(&fund, 1).unwrap();

        // Stake the validator minimum (+fee).
        let mut stake = Transaction::new(
            Some(address.clone()),
            STAKING_POOL_ACCOUNT.to_string(),
            stake_amount,
            TransactionType::Stake,
        );
        stake.nonce = 1;
        stake.public_key = Some(public_key.clone());
        stake.vrf_public_key =
            Some(crate::consensus::vrf::derive_vrf_public_key(&private_key).unwrap());
        state.apply_transaction(&stake, 1).unwrap();
        assert_eq!(state.validator_set().len(), 2);
        assert_eq!(state.balance_of(&address), funded - stake_amount - TX_FEE);

        // Withdraw the full stake (+fee) at height 2: funds enter
        // UNBONDING, they are NOT immediately spendable.
        let mut withdraw = Transaction::new(
            Some(address.clone()),
            address.clone(),
            stake_amount,
            TransactionType::Withdraw,
        );
        withdraw.nonce = 2;
        let message = Transaction::withdraw_signing_message(&address, stake_amount, 2);
        withdraw.signature = Some(pos::sign_message(&message, &private_key).unwrap());
        state.apply_transaction(&withdraw, 2).unwrap();

        assert_eq!(state.balance_of(&address), funded - stake_amount - 2 * TX_FEE);
        assert_eq!(state.unbonding_total(&address), stake_amount);
        // Exited from the active set, but keys retained while unbonding.
        assert_eq!(state.validator_set().len(), 1);
        assert!(state.stakers.contains_key(&address));

        // Not released before maturity.
        state.end_block(2 + UNBONDING_BLOCKS - 1, &treasury);
        assert_eq!(state.unbonding_total(&address), stake_amount);

        // Released at maturity; the fully exited validator entry is gone.
        state.end_block(2 + UNBONDING_BLOCKS, &treasury);
        assert_eq!(state.unbonding_total(&address), 0);
        assert_eq!(state.balance_of(&address), funded - 2 * TX_FEE);
        assert!(!state.stakers.contains_key(&address));
    }

    #[test]
    fn stake_below_the_validator_minimum_is_rejected() {
        let (mut state, treasury, _, _) = genesis_state();
        let (address, public_key, private_key) = wallet(2);

        let mut fund = Transaction::new(
            Some(treasury.clone()),
            address.clone(),
            MIN_VALIDATOR_STAKE,
            TransactionType::Transfer,
        );
        fund.nonce = 1;
        state.apply_transaction(&fund, 1).unwrap();

        let mut stake = Transaction::new(
            Some(address.clone()),
            STAKING_POOL_ACCOUNT.to_string(),
            MIN_VALIDATOR_STAKE - 1,
            TransactionType::Stake,
        );
        stake.nonce = 1;
        stake.public_key = Some(public_key.clone());
        stake.vrf_public_key =
            Some(crate::consensus::vrf::derive_vrf_public_key(&private_key).unwrap());
        let err = state.apply_transaction(&stake, 1).unwrap_err();
        assert!(err.contains("validator minimum"), "{err}");
        assert_eq!(state.validator_set().len(), 1);
    }

    #[test]
    fn partial_withdrawal_below_the_minimum_is_rejected() {
        let (mut state, treasury, _, treasury_key) = genesis_state();
        let _ = treasury;

        // Treasury holds the genesis stake; withdrawing all but a sliver
        // would leave a sub-minimum validator — rejected. Full exit is fine.
        let leave_dust = GENESIS_VALIDATOR_STAKE - MIN_VALIDATOR_STAKE + 1;
        let (treasury_addr, ..) = wallet(1);
        let mut withdraw = Transaction::new(
            Some(treasury_addr.clone()),
            treasury_addr.clone(),
            leave_dust,
            TransactionType::Withdraw,
        );
        withdraw.nonce = 1;
        let message = Transaction::withdraw_signing_message(&treasury_addr, leave_dust, 1);
        withdraw.signature = Some(pos::sign_message(&message, &treasury_key).unwrap());
        let err = state.apply_transaction(&withdraw, 1).unwrap_err();
        assert!(err.contains("validator minimum"), "{err}");
    }

    #[test]
    fn allowlist_gates_new_validator_registration() {
        let (t_addr, t_pub, t_priv) = wallet(1);
        let t_vrf = crate::consensus::vrf::derive_vrf_public_key(&t_priv).unwrap();
        let (allowed_addr, allowed_pub, allowed_key) = wallet(2);
        let (outsider_addr, outsider_pub, outsider_key) = wallet(3);

        // Genesis with an allowlist naming only wallet(2).
        let mut state = ChainState::genesis(
            &t_addr,
            Some(&t_pub),
            Some(&t_vrf),
            TEST_SUPPLY,
            &[allowed_addr.clone()],
        );

        // Fund both candidates.
        let funded = MIN_VALIDATOR_STAKE * 2;
        for (i, dest) in [(1u64, &allowed_addr), (2u64, &outsider_addr)] {
            let mut fund = Transaction::new(
                Some(t_addr.clone()),
                dest.to_string(),
                funded,
                TransactionType::Transfer,
            );
            fund.nonce = i;
            state.apply_transaction(&fund, 1).unwrap();
        }

        let make_stake = |addr: &str, pubkey: &str, key: &str| {
            let mut stake = Transaction::new(
                Some(addr.to_string()),
                STAKING_POOL_ACCOUNT.to_string(),
                MIN_VALIDATOR_STAKE,
                TransactionType::Stake,
            );
            stake.nonce = 1;
            stake.public_key = Some(pubkey.to_string());
            stake.vrf_public_key =
                Some(crate::consensus::vrf::derive_vrf_public_key(key).unwrap());
            stake
        };

        // An address NOT on the allowlist cannot join the validator set.
        let err = state
            .apply_transaction(&make_stake(&outsider_addr, &outsider_pub, &outsider_key), 1)
            .unwrap_err();
        assert!(err.contains("allowlist"), "{err}");
        assert_eq!(state.validator_set().len(), 1);

        // An allowlisted address joins normally.
        state
            .apply_transaction(&make_stake(&allowed_addr, &allowed_pub, &allowed_key), 1)
            .unwrap();
        assert_eq!(state.validator_set().len(), 2);

        // Existing validators (the genesis treasury) may top up regardless.
        let mut top_up = Transaction::new(
            Some(t_addr.clone()),
            STAKING_POOL_ACCOUNT.to_string(),
            MIN_VALIDATOR_STAKE,
            TransactionType::Stake,
        );
        top_up.nonce = 3;
        top_up.public_key = Some(t_pub.clone());
        top_up.vrf_public_key = Some(t_vrf.clone());
        state.apply_transaction(&top_up, 1).unwrap();
        assert_eq!(
            state.stakers[&t_addr].stake,
            GENESIS_VALIDATOR_STAKE + MIN_VALIDATOR_STAKE
        );
    }

    #[test]
    fn vesting_releases_after_cliff_then_linearly() {
        let (mut state, treasury, _, _) = genesis_state();
        let recipient = "hkmteammember".to_string();
        let total = 1_000_000 * UNITS_PER_HKM; // 1M HKM lockup

        // Vest at height 10: cliff 100 blocks, full vest over 400 blocks.
        let mut vest = Transaction::new(
            Some(treasury.clone()),
            recipient.clone(),
            total,
            TransactionType::Vest,
        );
        vest.nonce = 1;
        vest.vesting_cliff_blocks = Some(100);
        vest.vesting_duration_blocks = Some(400);
        state.apply_transaction(&vest, 10).unwrap();

        // Locked in the pool, not spendable by the recipient.
        assert_eq!(state.balance_of(VESTING_POOL_ACCOUNT), total);
        assert_eq!(state.balance_of(&recipient), 0);

        // Before the cliff: nothing releases.
        state.end_block(10 + 99, &treasury);
        assert_eq!(state.balance_of(&recipient), 0);

        // At the cliff: everything accrued since start releases at once
        // (100/400 = 25%).
        state.end_block(10 + 100, &treasury);
        assert_eq!(state.balance_of(&recipient), total / 4);

        // Midway: linear accrual (50%).
        state.end_block(10 + 200, &treasury);
        assert_eq!(state.balance_of(&recipient), total / 2);

        // At the end: fully vested, pool empty, entry cleaned up.
        state.end_block(10 + 400, &treasury);
        assert_eq!(state.balance_of(&recipient), total);
        assert_eq!(state.balance_of(VESTING_POOL_ACCOUNT), 0);
        assert!(state.vesting.is_empty());

        // Supply is conserved: vesting moves tokens, it never mints.
        assert_eq!(state.total_supply, TEST_SUPPLY);
    }

    #[test]
    fn vesting_rejects_bad_schedules_and_overdrafts() {
        let (mut state, treasury, _, _) = genesis_state();

        // Cliff beyond duration: rejected.
        let mut bad = Transaction::new(
            Some(treasury.clone()),
            "hkmsomeone".to_string(),
            100 * UNITS_PER_HKM,
            TransactionType::Vest,
        );
        bad.nonce = 1;
        bad.vesting_cliff_blocks = Some(500);
        bad.vesting_duration_blocks = Some(400);
        assert!(state.apply_transaction(&bad, 1).is_err());

        // Overdraft: a pauper cannot vest what they do not hold.
        let (pauper, ..) = wallet(9);
        let mut broke = Transaction::new(
            Some(pauper),
            "hkmsomeone".to_string(),
            100 * UNITS_PER_HKM,
            TransactionType::Vest,
        );
        broke.nonce = 1;
        broke.vesting_cliff_blocks = Some(0);
        broke.vesting_duration_blocks = Some(10);
        assert!(state.apply_transaction(&broke, 1).is_err());
    }

    #[test]
    fn slash_reaches_unbonding_stake_and_respects_window() {
        let (mut state, treasury, treasury_pub, treasury_key) = genesis_state();
        let _ = treasury_pub;

        // Treasury withdraws 400k HKM of its 1M HKM genesis stake at height 5.
        let withdrawn = 400_000 * UNITS_PER_HKM;
        let mut withdraw = Transaction::new(
            Some(treasury.clone()),
            treasury.clone(),
            withdrawn,
            TransactionType::Withdraw,
        );
        withdraw.nonce = 1;
        let message = Transaction::withdraw_signing_message(&treasury, withdrawn, 1);
        withdraw.signature = Some(pos::sign_message(&message, &treasury_key).unwrap());
        state.apply_transaction(&withdraw, 5).unwrap();
        assert_eq!(state.unbonding_total(&treasury), withdrawn);

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

        // Inside the window: slashes 10% of bonded (600k) + unbonding (400k)
        // = 100k HKM.
        let slashed = 100_000 * UNITS_PER_HKM;
        let pool_before = state.balance_of(STAKING_POOL_ACCOUNT);
        state.apply_transaction(&slash, 8).unwrap();
        assert_eq!(state.burned, slashed);
        assert_eq!(
            state.balance_of(STAKING_POOL_ACCOUNT),
            pool_before - slashed
        );
        // Bonded stake absorbs the deduction first.
        assert_eq!(state.stakers[&treasury].stake, 500_000 * UNITS_PER_HKM);
        assert_eq!(state.unbonding_total(&treasury), withdrawn);
    }

    #[test]
    fn base_fee_rises_with_congestion_and_falls_when_idle() {
        let mid = 8 * TX_FEE; // comfortably above the floor
        // Above target → rises (bounded to +1/8 per step at minimum +1).
        let up = next_base_fee(mid, BASE_FEE_TARGET_TXS + BASE_FEE_TARGET_TXS);
        assert!(up > mid);
        assert!(up <= mid + mid / 8 + 1);
        // Below target → falls but never past the floor.
        let down = next_base_fee(mid, 0);
        assert!(down < mid && down >= TX_FEE);
        // At the floor, an idle block keeps it at the floor.
        assert_eq!(next_base_fee(TX_FEE, 0), TX_FEE);
        // At target → unchanged.
        assert_eq!(next_base_fee(mid, BASE_FEE_TARGET_TXS), mid);
        // Never exceeds the ceiling.
        assert!(next_base_fee(BASE_FEE_MAX, 10_000) <= BASE_FEE_MAX);
    }

    #[test]
    fn end_block_updates_base_fee_deterministically() {
        let (mut state, treasury, _, _) = genesis_state();
        // Simulate a full block: BASE_FEE_TARGET_TXS + 40 fee-paying txs by
        // seeding the pot directly (each paid base_fee).
        state.base_fee = 100;
        let fee_paying = BASE_FEE_TARGET_TXS + 40;
        state.fee_pot = state.base_fee * fee_paying;
        let expected = next_base_fee(100, fee_paying);
        state.end_block(1, &treasury);
        assert_eq!(state.base_fee, expected);
        assert!(state.base_fee > 100, "congested block should raise the fee");
        assert_eq!(state.fee_pot, 0);
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
        let reward = Transaction::new_reward(&treasury, 1);
        let supply_before = state.total_supply;
        state.apply_transaction(&reward, 1).unwrap();
        assert_eq!(state.total_supply, supply_before + BLOCK_REWARD);
    }
}
