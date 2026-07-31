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

/// Internal account custodying all AMM pool reserves (HKM and tokens). The
/// per-pool split lives in `ChainState::pools`; this account holds the
/// aggregate so HKM/token supply stays conserved.
pub const AMM_POOL_ACCOUNT: &str = "__amm_pool__";

/// LP shares permanently locked on first liquidity provision, preventing the
/// first-depositor share-inflation attack (Uniswap-v2's MINIMUM_LIQUIDITY).
pub const MINIMUM_LIQUIDITY: u64 = 1_000;

/// Default network identifier when none is configured. It deliberately says
/// "dev": a node whose network was never named should not look like mainnet
/// to anything reading its transactions.
pub const DEFAULT_CHAIN_ID: &str = "hikmalayer-dev";

/// Is this a well-formed native address (`hkm` + 40 lowercase hex)?
///
/// Balances are keyed by string, so without this check a recipient of
/// `"hkm13320…"` with one character wrong is simply a different account —
/// one nobody holds the key to. The transfer succeeds, the balance exists,
/// and the funds are gone. There is no checksum to catch it later and no way
/// to reverse it, so the only place to stop it is here.
pub fn is_valid_address(value: &str) -> bool {
    value.len() == 43
        && value.starts_with("hkm")
        && value[3..].bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

/// Integer square root (floor) for u128 — used to size initial LP shares as
/// sqrt(hkm * token), keeping the first provider's shares independent of the
/// pool's price.
fn isqrt_u128(n: u128) -> u128 {
    if n < 2 {
        return n;
    }
    let mut x = n;
    let mut y = x.div_ceil(2);
    while y < x {
        x = y;
        y = (x + n / x) / 2;
    }
    x
}

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

/// A native fungible token (HTS — Hikmalayer Token Standard): the ecosystem
/// asset primitive a DEX and dapps build on. Supply is fixed at creation
/// (only reducible by burning), so no issuer can silently inflate a token.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct TokenInfo {
    pub token_id: String,
    pub symbol: String,
    pub name: String,
    pub decimals: u32,
    pub total_supply: u64,
    pub creator: String,
    pub created_at_height: u64,
}

/// A constant-product AMM liquidity pool pairing a native token with HKM.
/// `reserve_hkm * reserve_token` is the invariant a swap preserves (net of
/// the fee, which stays in the reserves and accrues to LPs). `total_shares`
/// includes the permanently-locked MINIMUM_LIQUIDITY minted on creation.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct Pool {
    pub token_id: String,
    pub reserve_hkm: u64,
    pub reserve_token: u64,
    pub total_shares: u64,
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
    /// Native token registry (HTS): token_id → immutable token metadata.
    #[serde(default)]
    pub tokens: BTreeMap<String, TokenInfo>,
    /// Native token balances: token_id → (holder address → units). Nested
    /// BTreeMaps keep the state-root serialization canonical.
    #[serde(default)]
    pub token_balances: BTreeMap<String, BTreeMap<String, u64>>,
    /// AMM liquidity pools, keyed by the token they pair with HKM.
    #[serde(default)]
    pub pools: BTreeMap<String, Pool>,
    /// LP share balances: token_id → (provider address → shares).
    #[serde(default)]
    pub lp_shares: BTreeMap<String, BTreeMap<String, u64>>,
    /// Genesis-configured validator allowlist. When NON-EMPTY, only listed
    /// addresses may register a NEW stake (existing validators may top up);
    /// empty means permissionless staking. Set once at genesis and part of
    /// the state root, so every node enforces the identical policy — this
    /// is the honest "permissioned hybrid at launch" lever, opened later
    /// via a scheduled network upgrade.
    #[serde(default)]
    pub validator_allowlist: std::collections::BTreeSet<String>,
    /// This network's identifier, fixed at genesis and committed to by the
    /// state root. Every user transaction names the network it is for, and
    /// one signed for a different network is refused — so a transaction made
    /// while testing cannot be replayed against real funds.
    #[serde(default)]
    pub chain_id: String,
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
        Self::genesis_for_chain(
            DEFAULT_CHAIN_ID,
            treasury_address,
            treasury_public_key,
            treasury_vrf_public_key,
            initial_supply,
            validator_allowlist,
        )
    }

    /// Genesis for a named network.
    #[allow(clippy::too_many_arguments)]
    pub fn genesis_for_chain(
        chain_id: &str,
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
            chain_id: chain_id.to_string(),
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

    /// Native-token balance of `account` for `token_id`.
    pub fn token_balance_of(&self, token_id: &str, account: &str) -> u64 {
        self.token_balances
            .get(token_id)
            .and_then(|holders| holders.get(account))
            .copied()
            .unwrap_or(0)
    }

    /// Credit token units, refusing to wrap. A token may declare up to 18
    /// decimals, so its base units get close to the top of u64 far sooner
    /// than HKM's do.
    fn try_credit_token(
        &mut self,
        token_id: &str,
        account: &str,
        amount: u64,
    ) -> Result<(), String> {
        let entry = self
            .token_balances
            .entry(token_id.to_string())
            .or_default()
            .entry(account.to_string())
            .or_insert(0);
        *entry = entry.checked_add(amount).ok_or_else(|| {
            format!("Credit of {} to {} would overflow the balance", token_id, account)
        })?;
        Ok(())
    }

    /// Infallible token credit for amounts already bounded by this state
    /// machine (returning pool reserves it previously debited).
    fn credit_token(&mut self, token_id: &str, account: &str, amount: u64) {
        let entry = self
            .token_balances
            .entry(token_id.to_string())
            .or_default()
            .entry(account.to_string())
            .or_insert(0);
        *entry = entry.saturating_add(amount);
    }

    /// LP shares held by `account` in the pool for `token_id`.
    pub fn lp_shares_of(&self, token_id: &str, account: &str) -> u64 {
        self.lp_shares
            .get(token_id)
            .and_then(|holders| holders.get(account))
            .copied()
            .unwrap_or(0)
    }

    fn debit_token(&mut self, token_id: &str, account: &str, amount: u64) -> Result<(), String> {
        let balance = self.token_balance_of(token_id, account);
        if balance < amount {
            return Err(format!(
                "Insufficient token balance for {} in {}: has {}, needs {}",
                account, token_id, balance, amount
            ));
        }
        let holders = self
            .token_balances
            .get_mut(token_id)
            .ok_or_else(|| format!("Unknown token {}", token_id))?;
        let entry = holders.get_mut(account).unwrap();
        *entry -= amount;
        if *entry == 0 {
            holders.remove(account);
        }
        Ok(())
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

    /// Credit an account, refusing to wrap.
    ///
    /// Every balance-affecting operation in this state machine is checked.
    /// Unchecked `u64` arithmetic here is not a rounding concern: with
    /// overflow checks off — the default for a release build, which is what
    /// a validator actually runs — a wrap turns into minted supply or
    /// destroyed funds, silently and irreversibly. With them on it panics,
    /// which is a remote denial of service any account can trigger. Neither
    /// is acceptable, so overflow is a rejected transaction instead.
    fn try_credit(&mut self, account: &str, amount: u64) -> Result<(), String> {
        let entry = self.balances.entry(account.to_string()).or_insert(0);
        *entry = entry
            .checked_add(amount)
            .ok_or_else(|| format!("Credit to {} would overflow the balance", account))?;
        Ok(())
    }

    /// Infallible credit for amounts the caller has already bounded (block
    /// rewards, fee payouts, funds moving back out of a pool this state
    /// machine itself debited). Saturates rather than wrapping, so even a
    /// mistake upstream cannot manufacture supply.
    fn credit(&mut self, account: &str, amount: u64) {
        let entry = self.balances.entry(account.to_string()).or_insert(0);
        *entry = entry.saturating_add(amount);
    }

    /// `amount + fee`, refused rather than wrapped.
    ///
    /// This is the arithmetic an attacker aims at: pick `amount` so that
    /// `amount + fee` wraps to something small, and the balance check passes
    /// while the full amount is still credited elsewhere.
    fn total_debit(amount: u64, fee: u64) -> Result<u64, String> {
        amount
            .checked_add(fee)
            .ok_or_else(|| "Amount plus fee exceeds the maximum representable value".to_string())
    }

    /// Add to the fee pot without wrapping.
    fn collect_fee(&mut self, fee: u64) {
        self.fee_pot = self.fee_pot.saturating_add(fee);
    }

    /// Apply one transaction at `height`. Stateless validity (signature
    /// schemes, reward shape) is checked by `Transaction::verify_for_block`;
    /// this method enforces the stateful rules: nonces, balances + fees,
    /// stake accounting with unbonding, registered-key checks, and slashing.
    pub fn apply_transaction(&mut self, tx: &Transaction, height: u64) -> Result<(), String> {
        // Authorization is checked HERE, not left to the caller. Block
        // validation verifies every transaction before applying it, and this
        // repeats that work — deliberately. A state machine that only stays
        // safe because every caller remembered to verify first is one new
        // code path away from writing a forged transaction into the ledger,
        // and by then it is consensus.
        tx.verify_authorization()?;
        self.apply_verified(tx, height)
    }

    /// Refuse a transaction that was signed for a different network.
    ///
    /// The chain id is inside the signed message, so a foreign transaction
    /// cannot be re-labelled without invalidating its signature. This check
    /// is what makes that binding mean something.
    ///
    /// Block-scoped transactions (Reward, Slash) carry no sender signature
    /// and are produced by this chain's own validators, so they are exempt.
    pub fn verify_chain_scope(&self, tx: &Transaction) -> Result<(), String> {
        if matches!(
            tx.transaction_type,
            TransactionType::Reward | TransactionType::Slash
        ) {
            return Ok(());
        }
        if tx.chain_id != self.chain_id {
            return Err(format!(
                "Transaction is for network '{}', this chain is '{}'",
                tx.chain_id, self.chain_id
            ));
        }
        Ok(())
    }

    /// Apply a transaction whose authorization has already been verified.
    ///
    /// Only for callers that have just run the full `verify_for_block` check
    /// — block validation and block production — where re-verifying every
    /// signature would double the cost of the hot path for no added safety.
    pub fn apply_verified(&mut self, tx: &Transaction, height: u64) -> Result<(), String> {
        // The network check lives HERE, not in `apply_transaction`, because
        // block validation applies transactions through this path. If it sat
        // one level up, a validator could put a transaction signed for
        // another network into a block and every node would accept it — the
        // signature is genuine, just for a different chain. That would defeat
        // the replay protection entirely at the only layer where it matters.
        //
        // It is a string comparison, not a signature check, so unlike
        // authorization there is nothing to gain by skipping it.
        self.verify_chain_scope(tx)?;
        match tx.transaction_type {
            TransactionType::Transfer => {
                let from = tx
                    .from
                    .as_ref()
                    .ok_or_else(|| "Transfer missing sender".to_string())?;
                if !is_valid_address(&tx.to) {
                    return Err(format!(
                        "Transfer recipient '{}' is not a valid hkm address",
                        tx.to
                    ));
                }
                self.consume_nonce(from, tx.nonce)?;
                let fee = self.base_fee;
                self.debit(from, Self::total_debit(tx.amount, fee)?)?;
                self.try_credit(&tx.to, tx.amount)?;
                self.collect_fee(fee);
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
                self.debit(from, Self::total_debit(tx.amount, fee)?)?;
                self.try_credit(STAKING_POOL_ACCOUNT, tx.amount)?;
                self.collect_fee(fee);
                let entry = self.stakers.entry(from.clone()).or_default();
                entry.stake = entry
                    .stake
                    .checked_add(tx.amount)
                    .ok_or_else(|| "Stake would overflow".to_string())?;
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
                // Scoped to this network like every other signature, so a
                // withdrawal signed on one chain cannot unbond stake on
                // another.
                let message = Transaction::scoped_signing_message(
                    &tx.chain_id,
                    &Transaction::withdraw_signing_message(from, tx.amount, tx.nonce),
                );
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
                self.collect_fee(fee);
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
                if !is_valid_address(&tx.to) {
                    return Err(format!(
                        "Vest beneficiary '{}' is not a valid hkm address",
                        tx.to
                    ));
                }
                self.consume_nonce(from, tx.nonce)?;
                let fee = self.base_fee;
                self.debit(from, Self::total_debit(tx.amount, fee)?)?;
                self.try_credit(VESTING_POOL_ACCOUNT, tx.amount)?;
                self.collect_fee(fee);
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
                // `Transaction::verify_for_block` already bounds this to the
                // emission schedule. Saturating here means even a bug
                // upstream cannot wrap the supply counter around to zero.
                self.credit(&tx.to, tx.amount);
                self.total_supply = self.total_supply.saturating_add(tx.amount);
                Ok(())
            }
            TransactionType::TokenCreate => {
                let from = tx
                    .from
                    .as_ref()
                    .ok_or_else(|| "TokenCreate missing creator".to_string())?;
                let action = tx
                    .token
                    .as_ref()
                    .ok_or_else(|| "TokenCreate missing token action".to_string())?;
                let token_id =
                    crate::blockchain::transaction::derive_token_id(from, &action.symbol, tx.nonce);
                if self.tokens.contains_key(&token_id) {
                    return Err(format!("Token {} already exists", token_id));
                }
                // Fee is charged in HKM (ties token issuance to HKM demand
                // and gates spam); the token's own supply is separate.
                self.consume_nonce(from, tx.nonce)?;
                let fee = self.base_fee;
                self.debit(from, fee)?;
                self.collect_fee(fee);

                self.tokens.insert(
                    token_id.clone(),
                    TokenInfo {
                        token_id: token_id.clone(),
                        symbol: action.symbol.clone(),
                        name: action.name.clone(),
                        decimals: action.decimals,
                        total_supply: tx.amount,
                        creator: from.clone(),
                        created_at_height: height,
                    },
                );
                self.try_credit_token(&token_id, from, tx.amount)?;
                Ok(())
            }
            TransactionType::TokenTransfer => {
                let from = tx
                    .from
                    .as_ref()
                    .ok_or_else(|| "TokenTransfer missing sender".to_string())?;
                let action = tx
                    .token
                    .as_ref()
                    .ok_or_else(|| "TokenTransfer missing token action".to_string())?;
                if !self.tokens.contains_key(&action.token_id) {
                    return Err(format!("Unknown token {}", action.token_id));
                }
                if !is_valid_address(&tx.to) {
                    return Err(format!(
                        "TokenTransfer recipient '{}' is not a valid hkm address",
                        tx.to
                    ));
                }
                self.consume_nonce(from, tx.nonce)?;
                let fee = self.base_fee;
                self.debit(from, fee)?; // HKM fee
                self.collect_fee(fee);
                self.debit_token(&action.token_id, from, tx.amount)?;
                self.try_credit_token(&action.token_id, &tx.to, tx.amount)?;
                Ok(())
            }
            TransactionType::TokenBurn => {
                let from = tx
                    .from
                    .as_ref()
                    .ok_or_else(|| "TokenBurn missing sender".to_string())?;
                let action = tx
                    .token
                    .as_ref()
                    .ok_or_else(|| "TokenBurn missing token action".to_string())?;
                if !self.tokens.contains_key(&action.token_id) {
                    return Err(format!("Unknown token {}", action.token_id));
                }
                self.consume_nonce(from, tx.nonce)?;
                let fee = self.base_fee;
                self.debit(from, fee)?; // HKM fee
                self.collect_fee(fee);
                self.debit_token(&action.token_id, from, tx.amount)?;
                // Reduce the token's recorded supply to match.
                if let Some(info) = self.tokens.get_mut(&action.token_id) {
                    info.total_supply = info.total_supply.saturating_sub(tx.amount);
                }
                Ok(())
            }
            TransactionType::AddLiquidity => {
                let from = tx
                    .from
                    .as_ref()
                    .ok_or_else(|| "AddLiquidity missing sender".to_string())?;
                let action = tx
                    .amm
                    .as_ref()
                    .ok_or_else(|| "AddLiquidity missing amm action".to_string())?;
                if !self.tokens.contains_key(&action.token_id) {
                    return Err(format!("Unknown token {}", action.token_id));
                }
                let existing = self.pools.get(&action.token_id).cloned().unwrap_or_default();

                // Determine the amounts actually used and the shares minted.
                let (use_hkm, use_token, minted, first) = if existing.total_shares == 0 {
                    // First provider sets the price and mints sqrt(hkm*token).
                    let product =
                        (action.amount_hkm as u128).saturating_mul(action.amount_token as u128);
                    let shares0 = isqrt_u128(product);
                    if shares0 <= MINIMUM_LIQUIDITY as u128 {
                        return Err("Initial liquidity is below the minimum".to_string());
                    }
                    let minted = (shares0 - MINIMUM_LIQUIDITY as u128) as u64;
                    (action.amount_hkm, action.amount_token, minted, true)
                } else {
                    // Preserve the pool ratio; use whichever side binds.
                    let rh = existing.reserve_hkm as u128;
                    let rt = existing.reserve_token as u128;
                    let token_optimal =
                        ((action.amount_hkm as u128) * rt / rh) as u64;
                    let (use_hkm, use_token) = if token_optimal <= action.amount_token {
                        (action.amount_hkm, token_optimal)
                    } else {
                        let hkm_optimal = ((action.amount_token as u128) * rh / rt) as u64;
                        (hkm_optimal, action.amount_token)
                    };
                    if use_hkm == 0 || use_token == 0 {
                        return Err("AddLiquidity amounts round to zero".to_string());
                    }
                    let ts = existing.total_shares as u128;
                    let shares_hkm = (use_hkm as u128) * ts / rh;
                    let shares_token = (use_token as u128) * ts / rt;
                    (use_hkm, use_token, shares_hkm.min(shares_token) as u64, false)
                };

                if minted == 0 || minted < action.min_shares {
                    return Err(format!(
                        "AddLiquidity slippage: {} shares minted, {} required",
                        minted, action.min_shares
                    ));
                }

                self.consume_nonce(from, tx.nonce)?;
                let fee = self.base_fee;
                self.debit(from, Self::total_debit(use_hkm, fee)?)?;
                self.collect_fee(fee);
                self.try_credit(AMM_POOL_ACCOUNT, use_hkm)?;
                self.debit_token(&action.token_id, from, use_token)?;
                self.try_credit_token(&action.token_id, AMM_POOL_ACCOUNT, use_token)?;

                let pool = self.pools.entry(action.token_id.clone()).or_insert(Pool {
                    token_id: action.token_id.clone(),
                    ..Default::default()
                });
                let minted_total = if first {
                    minted
                        .checked_add(MINIMUM_LIQUIDITY)
                        .ok_or_else(|| "LP shares would overflow".to_string())?
                } else {
                    minted
                };
                let next_hkm = pool
                    .reserve_hkm
                    .checked_add(use_hkm)
                    .ok_or_else(|| "Pool HKM reserve would overflow".to_string())?;
                let next_token = pool
                    .reserve_token
                    .checked_add(use_token)
                    .ok_or_else(|| "Pool token reserve would overflow".to_string())?;
                let next_shares = pool
                    .total_shares
                    .checked_add(minted_total)
                    .ok_or_else(|| "Pool LP shares would overflow".to_string())?;
                pool.reserve_hkm = next_hkm;
                pool.reserve_token = next_token;
                pool.total_shares = next_shares;

                let holding = self
                    .lp_shares
                    .entry(action.token_id.clone())
                    .or_default()
                    .entry(from.clone())
                    .or_insert(0);
                *holding = holding
                    .checked_add(minted)
                    .ok_or_else(|| "LP position would overflow".to_string())?;
                Ok(())
            }
            TransactionType::RemoveLiquidity => {
                let from = tx
                    .from
                    .as_ref()
                    .ok_or_else(|| "RemoveLiquidity missing sender".to_string())?;
                let action = tx
                    .amm
                    .as_ref()
                    .ok_or_else(|| "RemoveLiquidity missing amm action".to_string())?;
                let pool = self
                    .pools
                    .get(&action.token_id)
                    .cloned()
                    .ok_or_else(|| format!("No pool for {}", action.token_id))?;
                let held = self.lp_shares_of(&action.token_id, from);
                if held < action.shares {
                    return Err(format!(
                        "Insufficient LP shares: has {}, needs {}",
                        held, action.shares
                    ));
                }
                let ts = pool.total_shares as u128;
                let amount_hkm = ((action.shares as u128) * pool.reserve_hkm as u128 / ts) as u64;
                let amount_token =
                    ((action.shares as u128) * pool.reserve_token as u128 / ts) as u64;
                if amount_hkm == 0 || amount_token == 0 {
                    return Err("RemoveLiquidity amounts round to zero".to_string());
                }
                if amount_hkm < action.min_hkm || amount_token < action.min_token {
                    return Err("RemoveLiquidity slippage bound not met".to_string());
                }

                self.consume_nonce(from, tx.nonce)?;
                let fee = self.base_fee;
                self.debit(from, fee)?; // fee from liquid HKM
                self.collect_fee(fee);

                // Burn shares, return the underlying assets.
                let holders = self.lp_shares.get_mut(&action.token_id).unwrap();
                let entry = holders.get_mut(from).unwrap();
                *entry -= action.shares;
                if *entry == 0 {
                    holders.remove(from);
                }
                self.debit(AMM_POOL_ACCOUNT, amount_hkm)?;
                self.credit(from, amount_hkm);
                self.debit_token(&action.token_id, AMM_POOL_ACCOUNT, amount_token)?;
                self.credit_token(&action.token_id, from, amount_token);

                let pool = self.pools.get_mut(&action.token_id).unwrap();
                pool.reserve_hkm -= amount_hkm;
                pool.reserve_token -= amount_token;
                pool.total_shares -= action.shares;
                Ok(())
            }
            TransactionType::Swap => {
                let from = tx
                    .from
                    .as_ref()
                    .ok_or_else(|| "Swap missing sender".to_string())?;
                let action = tx
                    .amm
                    .as_ref()
                    .ok_or_else(|| "Swap missing amm action".to_string())?;
                let pool = self
                    .pools
                    .get(&action.token_id)
                    .cloned()
                    .ok_or_else(|| format!("No pool for {}", action.token_id))?;
                if pool.reserve_hkm == 0 || pool.reserve_token == 0 {
                    return Err("Pool has no liquidity".to_string());
                }
                let (reserve_in, reserve_out) = if action.hkm_to_token {
                    (pool.reserve_hkm, pool.reserve_token)
                } else {
                    (pool.reserve_token, pool.reserve_hkm)
                };
                // Constant-product with a 0.3% fee kept in the pool. u128
                // intermediates, checked so a pathologically large swap is
                // rejected rather than overflowing.
                let fee_num = crate::blockchain::transaction::AMM_FEE_DENOM
                    - crate::blockchain::transaction::SWAP_FEE_BPS;
                let amount_in_with_fee = (action.amount_in as u128)
                    .checked_mul(fee_num as u128)
                    .ok_or_else(|| "Swap amount too large".to_string())?;
                let numerator = amount_in_with_fee
                    .checked_mul(reserve_out as u128)
                    .ok_or_else(|| "Swap amount too large".to_string())?;
                let denominator = (reserve_in as u128)
                    .checked_mul(crate::blockchain::transaction::AMM_FEE_DENOM as u128)
                    .ok_or_else(|| "Pool reserve too large".to_string())?
                    + amount_in_with_fee;
                let amount_out = (numerator / denominator) as u64;
                if amount_out == 0 || amount_out >= reserve_out {
                    return Err("Swap produces no output or drains the pool".to_string());
                }
                if amount_out < action.min_out {
                    return Err(format!(
                        "Swap slippage: {} out, {} required",
                        amount_out, action.min_out
                    ));
                }

                self.consume_nonce(from, tx.nonce)?;
                let fee = self.base_fee;

                if action.hkm_to_token {
                    self.debit(from, Self::total_debit(action.amount_in, fee)?)?;
                    self.collect_fee(fee);
                    self.try_credit(AMM_POOL_ACCOUNT, action.amount_in)?;
                    self.debit_token(&action.token_id, AMM_POOL_ACCOUNT, amount_out)?;
                    self.credit_token(&action.token_id, from, amount_out);
                    let pool = self.pools.get_mut(&action.token_id).unwrap();
                    pool.reserve_hkm = pool
                        .reserve_hkm
                        .checked_add(action.amount_in)
                        .ok_or_else(|| "Pool HKM reserve would overflow".to_string())?;
                    pool.reserve_token -= amount_out;
                } else {
                    self.debit(from, fee)?; // HKM fee only
                    self.collect_fee(fee);
                    self.debit_token(&action.token_id, from, action.amount_in)?;
                    self.try_credit_token(&action.token_id, AMM_POOL_ACCOUNT, action.amount_in)?;
                    self.debit(AMM_POOL_ACCOUNT, amount_out)?;
                    self.credit(from, amount_out);
                    let pool = self.pools.get_mut(&action.token_id).unwrap();
                    pool.reserve_token = pool
                        .reserve_token
                        .checked_add(action.amount_in)
                        .ok_or_else(|| "Pool token reserve would overflow".to_string())?;
                    pool.reserve_hkm -= amount_out;
                }
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
                self.burned = self.burned.saturating_add(slashed);
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
    /// A transaction scoped to this test network.
    ///
    /// Every real transaction names the network it is for, and the state
    /// machine refuses one that does not match. Tests build transactions by
    /// hand, so they use this instead of `Transaction::new` directly.
    fn test_tx(
        from: Option<String>,
        to: String,
        amount: u64,
        kind: TransactionType,
    ) -> Transaction {
        Transaction::new(from, to, amount, kind).for_chain(DEFAULT_CHAIN_ID)
    }
    use super::*;
    use crate::blockchain::transaction::{AmmAction, BLOCK_REWARD};

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

    /// The supply is a consensus invariant: outside `Reward` (the emission
    /// schedule) and `TokenCreate`, no transaction may change how much HKM
    /// exists. An attacker who can break that mints money.
    fn hkm_in_existence(state: &ChainState) -> u128 {
        state.balances.values().map(|b| *b as u128).sum()
    }

    /// A transfer whose `amount + fee` overflows u64 must be rejected.
    ///
    /// Without checked arithmetic this is catastrophic. In a release build
    /// (overflow checks off) `amount + fee` wraps to a small number, so the
    /// balance check passes trivially — and then the FULL amount is credited
    /// to the recipient. An account holding nothing mints ~1.8×10^19 base
    /// units from thin air. In a debug build the same expression panics,
    /// which halts the node instead: a crash rather than a theft, but still
    /// a remote denial of service reachable by any account.
    #[test]
    fn transfer_amount_that_overflows_the_fee_cannot_mint_supply() {
        let (mut state, treasury, _, _) = genesis_state();
        let (attacker, ..) = wallet(7);
        // The attacker starts with a trivial balance.
        state.credit(&attacker, TX_FEE * 10);

        let supply_before = hkm_in_existence(&state);
        let attacker_before = state.balance_of(&attacker);
        let victim_before = state.balance_of(&treasury);

        // Chosen so `amount + base_fee` wraps to exactly zero.
        let amount = u64::MAX - state.base_fee + 1;
        let mut tx = test_tx(
            Some(attacker.clone()),
            treasury.clone(),
            amount,
            TransactionType::Transfer,
        );
        tx.nonce = 1;

        assert!(
            state.apply_verified(&tx, 1).is_err(),
            "an overflowing transfer was accepted"
        );
        assert_eq!(hkm_in_existence(&state), supply_before, "supply changed");
        assert_eq!(state.balance_of(&attacker), attacker_before);
        assert_eq!(state.balance_of(&treasury), victim_before);
    }

    /// Same class of bug on the staking path: `amount + fee` overflowing
    /// would let an attacker register an enormous stake — and enormous stake
    /// is control of leader election.
    #[test]
    fn stake_amount_that_overflows_the_fee_cannot_mint_stake() {
        let (mut state, _, _, _) = genesis_state();
        let (attacker, public_key, private_key) = wallet(7);
        state.credit(&attacker, TX_FEE * 10);
        let vrf_key = crate::consensus::vrf::derive_vrf_public_key(&private_key).unwrap();

        let amount = u64::MAX - state.base_fee + 1;
        let mut tx = test_tx(
            Some(attacker.clone()),
            attacker.clone(),
            amount,
            TransactionType::Stake,
        );
        tx.nonce = 1;
        tx.public_key = Some(public_key);
        tx.vrf_public_key = Some(vrf_key);

        assert!(state.apply_verified(&tx, 1).is_err());
        assert_eq!(state.stakers.get(&attacker).map(|s| s.stake), None);
    }

    /// Crediting an account must never wrap its balance around to a small
    /// number, which would destroy funds, nor overflow into a larger one.
    #[test]
    fn crediting_beyond_u64_is_rejected_rather_than_wrapping() {
        let (mut state, ..) = genesis_state();
        let (holder, ..) = wallet(7);
        state.balances.insert(holder.clone(), u64::MAX - 5);
        assert!(state.try_credit(&holder, 10).is_err());
        assert_eq!(state.balance_of(&holder), u64::MAX - 5, "balance mutated");
    }

    /// Liquidity provision touches four running totals at once; an overflow
    /// in any of them corrupts the pool's accounting for everyone in it.
    #[test]
    fn add_liquidity_that_overflows_a_reserve_is_rejected() {
        let (mut state, treasury, _, _) = genesis_state();
        let token_id = "hkt0123456789abcdef0123456789abcdef01234567".to_string();
        state.tokens.insert(
            token_id.clone(),
            TokenInfo {
                token_id: token_id.clone(),
                symbol: "OVR".into(),
                name: "Overflow".into(),
                decimals: 0,
                total_supply: u64::MAX,
                creator: treasury.clone(),
                created_at_height: 0,
            },
        );
        state.credit_token(&token_id, &treasury, u64::MAX);
        // A pool already holding nearly the whole range.
        state.pools.insert(
            token_id.clone(),
            Pool {
                token_id: token_id.clone(),
                reserve_hkm: u64::MAX - 1,
                reserve_token: u64::MAX - 1,
                total_shares: 1_000_000,
            },
        );

        let mut tx = test_tx(
            Some(treasury.clone()),
            treasury.clone(),
            0,
            TransactionType::AddLiquidity,
        );
        tx.nonce = 1;
        tx.amm = Some(AmmAction {
            token_id: token_id.clone(),
            amount_hkm: u64::MAX,
            amount_token: u64::MAX,
            min_shares: 0,
            ..Default::default()
        });

        // Whatever the outcome, it must not be a wrapped reserve.
        let _ = state.apply_verified(&tx, 1);
        let pool = state.pools.get(&token_id).unwrap();
        assert!(
            pool.reserve_hkm >= u64::MAX - 1,
            "reserve wrapped to {}",
            pool.reserve_hkm
        );
    }

    #[test]
    fn recognizes_well_formed_addresses() {
        let (address, ..) = wallet(1);
        assert!(is_valid_address(&address));
        // Wrong length, wrong prefix, non-hex, and uppercase hex (which would
        // let one key have two spellings) are all refused.
        assert!(!is_valid_address(""));
        assert!(!is_valid_address("hkm"));
        assert!(!is_valid_address(&address[..42]));
        assert!(!is_valid_address(&format!("{}0", address)));
        assert!(!is_valid_address(&address.replace("hkm", "abc")));
        assert!(!is_valid_address(&address.to_uppercase()));
        assert!(!is_valid_address(&format!("hkm{}", "z".repeat(40))));
        assert!(!is_valid_address("typo-address"));
        // Internal pool accounts are not addressable by users either.
        assert!(!is_valid_address(STAKING_POOL_ACCOUNT));
        assert!(!is_valid_address(AMM_POOL_ACCOUNT));
    }

    /// Balances are keyed by string, so an unvalidated recipient means a
    /// mistyped address is simply a different account — one nobody can spend
    /// from. The transfer would "succeed" and the funds would be gone.
    #[test]
    fn transfer_to_a_malformed_address_is_rejected() {
        let (mut state, treasury, _, _) = genesis_state();
        for bad in ["typo-address", "", "hkmshort", STAKING_POOL_ACCOUNT] {
            let mut tx = test_tx(
                Some(treasury.clone()),
                bad.to_string(),
                100,
                TransactionType::Transfer,
            );
            tx.nonce = 1;
            let sender_before = state.balance_of(&treasury);
            let target_before = state.balance_of(bad);
            assert!(
                state.apply_verified(&tx, 1).is_err(),
                "accepted a transfer to {bad:?}"
            );
            // Rejected cleanly: nothing moved, and no nonce was consumed.
            assert_eq!(state.balance_of(&treasury), sender_before);
            assert_eq!(state.balance_of(bad), target_before);
        }
    }

    #[test]
    fn vesting_to_a_malformed_beneficiary_is_rejected() {
        let (mut state, treasury, _, _) = genesis_state();
        let mut vest = test_tx(
            Some(treasury),
            "not-an-address".to_string(),
            1_000,
            TransactionType::Vest,
        );
        vest.nonce = 1;
        vest.vesting_cliff_blocks = Some(10);
        vest.vesting_duration_blocks = Some(100);
        assert!(state.apply_verified(&vest, 1).is_err());
        assert_eq!(state.balance_of(VESTING_POOL_ACCOUNT), 0);
    }

    #[test]
    fn state_root_is_deterministic_and_sensitive() {
        let (state_a, ..) = genesis_state();
        let (mut state_b, ..) = genesis_state();
        assert_eq!(state_a.state_root(), state_b.state_root());
        state_b.credit("hkm02789abcdef0123456789abcdef0123456789abc", 1);
        assert_ne!(state_a.state_root(), state_b.state_root());
    }

    #[test]
    fn transfer_updates_balances_and_nonce() {
        let (mut state, treasury, _, _) = genesis_state();
        let mut tx = test_tx(
            Some(treasury.clone()),
            "hkm010123456789abcdef0123456789abcdef012345".to_string(),
            100,
            TransactionType::Transfer,
        );
        tx.nonce = 1;
        state.apply_verified(&tx, 1).unwrap();
        assert_eq!(state.balance_of("hkm010123456789abcdef0123456789abcdef012345"), 100);
        assert_eq!(state.nonce_of(&treasury), 1);

        // Same nonce cannot apply twice.
        assert!(state.apply_verified(&tx, 1).is_err());
    }

    #[test]
    fn transfer_rejects_overdraft() {
        let (mut state, ..) = genesis_state();
        let (poor, ..) = wallet(9);
        let mut tx = test_tx(
            Some(poor),
            "hkm010123456789abcdef0123456789abcdef012345".to_string(),
            1,
            TransactionType::Transfer,
        );
        tx.nonce = 1;
        assert!(state.apply_verified(&tx, 1).is_err());
    }

    #[test]
    fn stake_withdraw_unbonding_lifecycle() {
        let (mut state, treasury, _, _) = genesis_state();
        let (address, public_key, private_key) = wallet(2);

        let stake_amount = MIN_VALIDATOR_STAKE;
        let funded = stake_amount + 10 * TX_FEE;

        // Fund the new validator from treasury (sender pays the fee).
        let mut fund = test_tx(
            Some(treasury.clone()),
            address.clone(),
            funded,
            TransactionType::Transfer,
        );
        fund.nonce = 1;
        state.apply_verified(&fund, 1).unwrap();

        // Stake the validator minimum (+fee).
        let mut stake = test_tx(
            Some(address.clone()),
            STAKING_POOL_ACCOUNT.to_string(),
            stake_amount,
            TransactionType::Stake,
        );
        stake.nonce = 1;
        stake.public_key = Some(public_key.clone());
        stake.vrf_public_key =
            Some(crate::consensus::vrf::derive_vrf_public_key(&private_key).unwrap());
        state.apply_verified(&stake, 1).unwrap();
        assert_eq!(state.validator_set().len(), 2);
        assert_eq!(state.balance_of(&address), funded - stake_amount - TX_FEE);

        // Withdraw the full stake (+fee) at height 2: funds enter
        // UNBONDING, they are NOT immediately spendable.
        let mut withdraw = test_tx(
            Some(address.clone()),
            address.clone(),
            stake_amount,
            TransactionType::Withdraw,
        );
        withdraw.nonce = 2;
        let message = Transaction::withdraw_signing_message(&address, stake_amount, 2);
        withdraw.chain_id = DEFAULT_CHAIN_ID.to_string();
        withdraw.signature = Some(pos::sign_message(
            &Transaction::scoped_signing_message(DEFAULT_CHAIN_ID, &message),
            &private_key,
        ).unwrap());
        state.apply_verified(&withdraw, 2).unwrap();

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

        let mut fund = test_tx(
            Some(treasury.clone()),
            address.clone(),
            MIN_VALIDATOR_STAKE,
            TransactionType::Transfer,
        );
        fund.nonce = 1;
        state.apply_verified(&fund, 1).unwrap();

        let mut stake = test_tx(
            Some(address.clone()),
            STAKING_POOL_ACCOUNT.to_string(),
            MIN_VALIDATOR_STAKE - 1,
            TransactionType::Stake,
        );
        stake.nonce = 1;
        stake.public_key = Some(public_key.clone());
        stake.vrf_public_key =
            Some(crate::consensus::vrf::derive_vrf_public_key(&private_key).unwrap());
        let err = state.apply_verified(&stake, 1).unwrap_err();
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
        let mut withdraw = test_tx(
            Some(treasury_addr.clone()),
            treasury_addr.clone(),
            leave_dust,
            TransactionType::Withdraw,
        );
        withdraw.nonce = 1;
        let message = Transaction::withdraw_signing_message(&treasury_addr, leave_dust, 1);
        withdraw.chain_id = DEFAULT_CHAIN_ID.to_string();
        withdraw.signature = Some(pos::sign_message(
            &Transaction::scoped_signing_message(DEFAULT_CHAIN_ID, &message),
            &treasury_key,
        ).unwrap());
        let err = state.apply_verified(&withdraw, 1).unwrap_err();
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
            std::slice::from_ref(&allowed_addr),
        );

        // Fund both candidates.
        let funded = MIN_VALIDATOR_STAKE * 2;
        for (i, dest) in [(1u64, &allowed_addr), (2u64, &outsider_addr)] {
            let mut fund = test_tx(
                Some(t_addr.clone()),
                dest.to_string(),
                funded,
                TransactionType::Transfer,
            );
            fund.nonce = i;
            state.apply_verified(&fund, 1).unwrap();
        }

        let make_stake = |addr: &str, pubkey: &str, key: &str| {
            let mut stake = test_tx(
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
            .apply_verified(&make_stake(&outsider_addr, &outsider_pub, &outsider_key), 1)
            .unwrap_err();
        assert!(err.contains("allowlist"), "{err}");
        assert_eq!(state.validator_set().len(), 1);

        // An allowlisted address joins normally.
        state
            .apply_verified(&make_stake(&allowed_addr, &allowed_pub, &allowed_key), 1)
            .unwrap();
        assert_eq!(state.validator_set().len(), 2);

        // Existing validators (the genesis treasury) may top up regardless.
        let mut top_up = test_tx(
            Some(t_addr.clone()),
            STAKING_POOL_ACCOUNT.to_string(),
            MIN_VALIDATOR_STAKE,
            TransactionType::Stake,
        );
        top_up.nonce = 3;
        top_up.public_key = Some(t_pub.clone());
        top_up.vrf_public_key = Some(t_vrf.clone());
        state.apply_verified(&top_up, 1).unwrap();
        assert_eq!(
            state.stakers[&t_addr].stake,
            GENESIS_VALIDATOR_STAKE + MIN_VALIDATOR_STAKE
        );
    }

    #[test]
    fn vesting_releases_after_cliff_then_linearly() {
        let (mut state, treasury, _, _) = genesis_state();
        let (recipient, _, _) = wallet(9);
        let total = 1_000_000 * UNITS_PER_HKM; // 1M HKM lockup

        // Vest at height 10: cliff 100 blocks, full vest over 400 blocks.
        let mut vest = test_tx(
            Some(treasury.clone()),
            recipient.clone(),
            total,
            TransactionType::Vest,
        );
        vest.nonce = 1;
        vest.vesting_cliff_blocks = Some(100);
        vest.vesting_duration_blocks = Some(400);
        state.apply_verified(&vest, 10).unwrap();

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
    fn native_token_create_transfer_burn_lifecycle() {
        use crate::blockchain::transaction::{derive_token_id, TokenAction};
        let (mut state, treasury, _, _) = genesis_state();
        let (holder, ..) = wallet(4);

        // Create a token: 1,000,000 units (8 decimals) minted to the creator.
        let supply = 1_000_000u64;
        let mut create = test_tx(
            Some(treasury.clone()),
            String::new(),
            supply,
            TransactionType::TokenCreate,
        );
        create.nonce = 1;
        create.token = Some(TokenAction {
            token_id: String::new(),
            symbol: "HTEST".to_string(),
            name: "Hik Test".to_string(),
            decimals: 8,
        });
        let hkm_before = state.balance_of(&treasury);
        state.apply_verified(&create, 1).unwrap();

        let token_id = derive_token_id(&treasury, "HTEST", 1);
        assert!(state.tokens.contains_key(&token_id));
        assert_eq!(state.tokens[&token_id].total_supply, supply);
        assert_eq!(state.tokens[&token_id].decimals, 8);
        assert_eq!(state.token_balance_of(&token_id, &treasury), supply);
        // Creation charged an HKM base fee (no token minted to anyone else).
        assert_eq!(state.balance_of(&treasury), hkm_before - state.base_fee);

        // Transfer 250,000 units to a holder.
        let mut send = test_tx(
            Some(treasury.clone()),
            holder.clone(),
            250_000,
            TransactionType::TokenTransfer,
        );
        send.nonce = 2;
        send.token = Some(TokenAction {
            token_id: token_id.clone(),
            ..Default::default()
        });
        state.apply_verified(&send, 1).unwrap();
        assert_eq!(state.token_balance_of(&token_id, &holder), 250_000);
        assert_eq!(state.token_balance_of(&token_id, &treasury), 750_000);
        // Token supply is unchanged by transfers.
        assert_eq!(state.tokens[&token_id].total_supply, supply);

        // Burn 100,000 units from the treasury: supply drops accordingly.
        let mut burn = test_tx(
            Some(treasury.clone()),
            String::new(),
            100_000,
            TransactionType::TokenBurn,
        );
        burn.nonce = 3;
        burn.token = Some(TokenAction {
            token_id: token_id.clone(),
            ..Default::default()
        });
        state.apply_verified(&burn, 1).unwrap();
        assert_eq!(state.token_balance_of(&token_id, &treasury), 650_000);
        assert_eq!(state.tokens[&token_id].total_supply, supply - 100_000);

        // HKM supply is untouched by any native-token operation.
        assert_eq!(state.total_supply, TEST_SUPPLY);
    }

    /// Helper: create a token owned by `owner` and return its id.
    fn make_token(state: &mut ChainState, owner: &str, symbol: &str, supply: u64, nonce: u64) -> String {
        use crate::blockchain::transaction::TokenAction;
        let mut create = test_tx(
            Some(owner.to_string()),
            String::new(),
            supply,
            TransactionType::TokenCreate,
        );
        create.nonce = nonce;
        create.token = Some(TokenAction {
            symbol: symbol.to_string(),
            ..Default::default()
        });
        state.apply_verified(&create, 1).unwrap();
        crate::blockchain::transaction::derive_token_id(owner, symbol, nonce)
    }

    #[test]
    fn amm_add_swap_remove_lifecycle_conserves_supply() {
        use crate::blockchain::transaction::AmmAction;
        let (mut state, treasury, _, _) = genesis_state();
        let hkm_supply_before = state.total_supply;

        // Create a token and a HKM<->token pool.
        let token_id = make_token(&mut state, &treasury, "LPTOK", 10_000_000, 1);

        // Seed the pool: 1,000,000 HKM units + 1,000,000 token units.
        let mut add = test_tx(
            Some(treasury.clone()),
            String::new(),
            0,
            TransactionType::AddLiquidity,
        );
        add.nonce = 2;
        add.amm = Some(AmmAction {
            token_id: token_id.clone(),
            amount_hkm: 1_000_000,
            amount_token: 1_000_000,
            min_shares: 1,
            ..Default::default()
        });
        state.apply_verified(&add, 1).unwrap();

        let pool = state.pools[&token_id].clone();
        assert_eq!(pool.reserve_hkm, 1_000_000);
        assert_eq!(pool.reserve_token, 1_000_000);
        // Provider shares = sqrt(1e6*1e6) - MINIMUM_LIQUIDITY.
        let provider_shares = state.lp_shares_of(&token_id, &treasury);
        assert_eq!(provider_shares, 1_000_000 - MINIMUM_LIQUIDITY);
        assert_eq!(pool.total_shares, 1_000_000);
        // The pool custody account holds the reserves.
        assert_eq!(state.balance_of(AMM_POOL_ACCOUNT), 1_000_000);
        assert_eq!(state.token_balance_of(&token_id, AMM_POOL_ACCOUNT), 1_000_000);

        // Swap 100,000 HKM for the token. Constant-product with 0.3% fee:
        // out = 997*100000*1e6 / (1e6*1000 + 997*100000) ≈ 90,661.
        let k_before = pool.reserve_hkm as u128 * pool.reserve_token as u128;
        let mut swap = test_tx(
            Some(treasury.clone()),
            String::new(),
            0,
            TransactionType::Swap,
        );
        swap.nonce = 3;
        swap.amm = Some(AmmAction {
            token_id: token_id.clone(),
            amount_in: 100_000,
            hkm_to_token: true,
            min_out: 1,
            ..Default::default()
        });
        let token_before = state.token_balance_of(&token_id, &treasury);
        state.apply_verified(&swap, 1).unwrap();
        let received = state.token_balance_of(&token_id, &treasury) - token_before;
        assert_eq!(received, 90_661, "constant-product output");

        // The pool moved: more HKM, less token, and k grew (fee accrual).
        let pool2 = state.pools[&token_id].clone();
        assert_eq!(pool2.reserve_hkm, 1_100_000);
        assert_eq!(pool2.reserve_token, 1_000_000 - 90_661);
        let k_after = pool2.reserve_hkm as u128 * pool2.reserve_token as u128;
        assert!(k_after > k_before, "fee grows the invariant for LPs");

        // Remove all of the provider's liquidity; they get back reserves pro
        // rata (now HKM-heavier and token-lighter from the swap + the fee).
        let mut remove = test_tx(
            Some(treasury.clone()),
            String::new(),
            0,
            TransactionType::RemoveLiquidity,
        );
        remove.nonce = 4;
        remove.amm = Some(AmmAction {
            token_id: token_id.clone(),
            shares: provider_shares,
            min_hkm: 1,
            min_token: 1,
            ..Default::default()
        });
        state.apply_verified(&remove, 1).unwrap();
        assert_eq!(state.lp_shares_of(&token_id, &treasury), 0);
        // Only the locked MINIMUM_LIQUIDITY worth of reserves remains.
        let pool3 = state.pools[&token_id].clone();
        assert_eq!(pool3.total_shares, MINIMUM_LIQUIDITY);
        assert!(pool3.reserve_hkm > 0 && pool3.reserve_token > 0);

        // HKM total supply is conserved across every AMM operation (fees to
        // the validator stay in circulation; the pool never mints or burns).
        // Account for fees paid to the (unset) validator: they sit in fee_pot
        // until end_block, so total_supply is unchanged throughout.
        assert_eq!(state.total_supply, hkm_supply_before);
    }

    #[test]
    fn amm_enforces_slippage_and_liquidity_bounds() {
        use crate::blockchain::transaction::AmmAction;
        let (mut state, treasury, _, _) = genesis_state();
        let token_id = make_token(&mut state, &treasury, "SLIP", 10_000_000, 1);

        let mut add = test_tx(
            Some(treasury.clone()),
            String::new(),
            0,
            TransactionType::AddLiquidity,
        );
        add.nonce = 2;
        add.amm = Some(AmmAction {
            token_id: token_id.clone(),
            amount_hkm: 1_000_000,
            amount_token: 1_000_000,
            min_shares: 1,
            ..Default::default()
        });
        state.apply_verified(&add, 1).unwrap();

        // Swap with an unsatisfiable min_out is rejected.
        let mut swap = test_tx(
            Some(treasury.clone()),
            String::new(),
            0,
            TransactionType::Swap,
        );
        swap.nonce = 3;
        swap.amm = Some(AmmAction {
            token_id: token_id.clone(),
            amount_in: 100_000,
            hkm_to_token: true,
            min_out: 99_999_999, // absurd
            ..Default::default()
        });
        let err = state.apply_verified(&swap, 1).unwrap_err();
        assert!(err.contains("slippage"), "{err}");

        // Swap against a pool that does not exist is rejected.
        let mut ghost = test_tx(
            Some(treasury.clone()),
            String::new(),
            0,
            TransactionType::Swap,
        );
        ghost.nonce = 3;
        ghost.amm = Some(AmmAction {
            token_id: "hktnope".to_string(),
            amount_in: 1,
            hkm_to_token: true,
            min_out: 0,
            ..Default::default()
        });
        assert!(state.apply_verified(&ghost, 1).is_err());
    }

    #[test]
    fn native_token_rejects_overdraft_and_unknown_token() {
        use crate::blockchain::transaction::TokenAction;
        let (mut state, treasury, _, _) = genesis_state();

        // Transfer against a token that does not exist.
        let mut send = test_tx(
            Some(treasury.clone()),
            "hkm02789abcdef0123456789abcdef0123456789abc".to_string(),
            1,
            TransactionType::TokenTransfer,
        );
        send.nonce = 1;
        send.token = Some(TokenAction {
            token_id: "hktdeadbeef".to_string(),
            ..Default::default()
        });
        assert!(state.apply_verified(&send, 1).is_err());

        // Create a small token, then try to transfer more than held.
        let mut create = test_tx(
            Some(treasury.clone()),
            String::new(),
            100,
            TransactionType::TokenCreate,
        );
        create.nonce = 1;
        create.token = Some(TokenAction {
            token_id: String::new(),
            symbol: "SMALL".to_string(),
            name: String::new(),
            decimals: 0,
        });
        state.apply_verified(&create, 1).unwrap();
        let token_id = crate::blockchain::transaction::derive_token_id(&treasury, "SMALL", 1);

        let mut overspend = test_tx(
            Some(treasury.clone()),
            "hkm02789abcdef0123456789abcdef0123456789abc".to_string(),
            101,
            TransactionType::TokenTransfer,
        );
        overspend.nonce = 2;
        overspend.token = Some(TokenAction {
            token_id: token_id.clone(),
            ..Default::default()
        });
        let err = state.apply_verified(&overspend, 1).unwrap_err();
        assert!(err.contains("Insufficient token balance"), "{err}");
    }

    #[test]
    fn vesting_rejects_bad_schedules_and_overdrafts() {
        let (mut state, treasury, _, _) = genesis_state();

        // Cliff beyond duration: rejected.
        let mut bad = test_tx(
            Some(treasury.clone()),
            "hkm02789abcdef0123456789abcdef0123456789abc".to_string(),
            100 * UNITS_PER_HKM,
            TransactionType::Vest,
        );
        bad.nonce = 1;
        bad.vesting_cliff_blocks = Some(500);
        bad.vesting_duration_blocks = Some(400);
        assert!(state.apply_verified(&bad, 1).is_err());

        // Overdraft: a pauper cannot vest what they do not hold.
        let (pauper, ..) = wallet(9);
        let mut broke = test_tx(
            Some(pauper),
            "hkm02789abcdef0123456789abcdef0123456789abc".to_string(),
            100 * UNITS_PER_HKM,
            TransactionType::Vest,
        );
        broke.nonce = 1;
        broke.vesting_cliff_blocks = Some(0);
        broke.vesting_duration_blocks = Some(10);
        assert!(state.apply_verified(&broke, 1).is_err());
    }

    #[test]
    fn slash_reaches_unbonding_stake_and_respects_window() {
        let (mut state, treasury, treasury_pub, treasury_key) = genesis_state();
        let _ = treasury_pub;

        // Treasury withdraws 400k HKM of its 1M HKM genesis stake at height 5.
        let withdrawn = 400_000 * UNITS_PER_HKM;
        let mut withdraw = test_tx(
            Some(treasury.clone()),
            treasury.clone(),
            withdrawn,
            TransactionType::Withdraw,
        );
        withdraw.nonce = 1;
        let message = Transaction::withdraw_signing_message(&treasury, withdrawn, 1);
        withdraw.chain_id = DEFAULT_CHAIN_ID.to_string();
        withdraw.signature = Some(pos::sign_message(
            &Transaction::scoped_signing_message(DEFAULT_CHAIN_ID, &message),
            &treasury_key,
        ).unwrap());
        state.apply_verified(&withdraw, 5).unwrap();
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
        let mut slash = test_tx(None, treasury.clone(), 0, TransactionType::Slash);
        slash.slash_proof = Some(SlashProof {
            block_a: make_block("a"),
            block_b: make_block("b"),
        });

        // Outside the window: rejected.
        let err = state
            .apply_verified(&slash, 7 + SLASHING_WINDOW_BLOCKS + 1)
            .unwrap_err();
        assert!(err.contains("window"), "{err}");

        // Inside the window: slashes 10% of bonded (600k) + unbonding (400k)
        // = 100k HKM.
        let slashed = 100_000 * UNITS_PER_HKM;
        let pool_before = state.balance_of(STAKING_POOL_ACCOUNT);
        state.apply_verified(&slash, 8).unwrap();
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
        let mut tx = test_tx(
            Some(treasury.clone()),
            "hkm010123456789abcdef0123456789abcdef012345".to_string(),
            100,
            TransactionType::Transfer,
        );
        tx.nonce = 1;
        state.apply_verified(&tx, 1).unwrap();
        assert_eq!(state.fee_pot, TX_FEE);

        state.end_block(1, "hkmvalidator");
        assert_eq!(state.fee_pot, 0);
        assert_eq!(state.balance_of("hkmvalidator"), TX_FEE);
    }

    #[test]
    fn withdraw_rejects_wrong_key() {
        let (mut state, treasury, _, _) = genesis_state();
        let (_, _, intruder_key) = wallet(7);
        let mut withdraw = test_tx(
            Some(treasury.clone()),
            treasury.clone(),
            10,
            TransactionType::Withdraw,
        );
        withdraw.nonce = 1;
        let message = Transaction::withdraw_signing_message(&treasury, 10, 1);
        withdraw.chain_id = DEFAULT_CHAIN_ID.to_string();
        withdraw.signature = Some(pos::sign_message(
            &Transaction::scoped_signing_message(DEFAULT_CHAIN_ID, &message),
            &intruder_key,
        ).unwrap());
        assert!(state.apply_verified(&withdraw, 1).is_err());
    }

    #[test]
    fn reward_mints_supply() {
        let (mut state, treasury, _, _) = genesis_state();
        let reward = Transaction::new_reward(&treasury, 1);
        let supply_before = state.total_supply;
        state.apply_verified(&reward, 1).unwrap();
        assert_eq!(state.total_supply, supply_before + BLOCK_REWARD);
    }
}
