use chrono::prelude::*;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::blockchain::block::Block;
use crate::consensus::{hybrid, pos};

/// HKM is denominated with 6 decimal places: all on-chain amounts are in
/// base units, and 1 HKM = 1,000,000 base units. Chosen so the ~100B HKM
/// supply (10^17 base units) keeps ~180x headroom under u64::MAX — no
/// balance or supply aggregate can overflow.
pub const DECIMALS: u32 = 6;
pub const UNITS_PER_HKM: u64 = 1_000_000;

/// Initial block reward: 3,700 HKM (height 1 through the first halving).
pub const BLOCK_REWARD: u64 = 3_700 * UNITS_PER_HKM;

/// Blocks between reward halvings — a Bitcoin-style deterministic emission
/// schedule. At the 15s block target, 9,500,000 blocks ≈ 4.5 years per
/// halving epoch, so the four largest epochs (the bulk of emission) span
/// ~18 years. Halving-phase emission sums to ~70B HKM, which with the 30B
/// HKM genesis allocation gives the ~100B HKM supply at maturity (30/70
/// premine/mined split).
pub const HALVING_INTERVAL: u64 = 9_500_000;

/// Tail emission floor: 50 HKM per block. Once halvings would push the
/// reward below this floor (epoch 8, ~32 years in), the reward stays here
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
    Transfer,     // Transfer tokens
    Reward,       // Block production reward
    Certificate,  // Anchor a certificate issuance
    Stake,        // Register / increase validator stake (on-chain)
    Withdraw,     // Reduce / exit validator stake (on-chain)
    Slash,        // Punish a proven equivocation (on-chain)
    Vest,         // Lock tokens for a recipient on a cliff + linear schedule
    TokenCreate,  // Issue a new native fungible token (ecosystem asset)
    TokenTransfer, // Move units of a native token between accounts
    TokenBurn,    // Destroy units of a native token from the sender
    AddLiquidity, // Deposit HKM + a token into an AMM pool for LP shares
    RemoveLiquidity, // Burn LP shares, withdraw the underlying HKM + token
    Swap,         // Swap HKM<->token against an AMM pool (constant product)
}

/// Upper bound on a vesting schedule's duration (~47 years at 15s blocks).
/// Bounds per-entry arithmetic and prevents nonsense schedules.
pub const MAX_VESTING_DURATION_BLOCKS: u64 = 100_000_000;

/// Native token (HTS — Hikmalayer Token Standard) limits. Symbols and names
/// are bounded so state stays compact; decimals cap mirrors ERC-20/EVM norms
/// so ecosystem tooling and a future DEX can display any token safely.
pub const MAX_TOKEN_SYMBOL_LEN: usize = 12;
pub const MAX_TOKEN_NAME_LEN: usize = 64;
pub const MAX_TOKEN_DECIMALS: u32 = 18;

/// Extra fields carried by the native-token transaction types. Which fields
/// are meaningful depends on the transaction type (mirrors how the amount
/// and recipient are reused across Stake/Withdraw): Create uses
/// symbol/name/decimals (+ tx.amount = initial supply); Transfer uses
/// token_id (+ tx.to = recipient, tx.amount = units); Burn uses token_id
/// (+ tx.amount = units).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct TokenAction {
    #[serde(default)]
    pub token_id: String,
    #[serde(default)]
    pub symbol: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub decimals: u32,
}

/// Deterministic token id: "hkt" + hex(SHA-256(creator:symbol:nonce)[..20]).
/// (creator, nonce) is globally unique because nonces are strictly
/// increasing per account, so no two Create transactions can collide.
pub fn derive_token_id(creator: &str, symbol: &str, nonce: u64) -> String {
    use sha2::{Digest, Sha256};
    let seed = format!("{}:{}:{}", creator, symbol, nonce);
    let digest = Sha256::digest(seed.as_bytes());
    format!("hkt{}", hex::encode(&digest[..20]))
}

/// AMM swap fee in basis points (0.30%), kept in the pool reserves and thus
/// accruing to liquidity providers — the same fee level as Uniswap v2.
pub const SWAP_FEE_BPS: u64 = 30;
pub const AMM_FEE_DENOM: u64 = 10_000;

/// Parameters for the AMM transaction types. Which fields matter depends on
/// the transaction type (mirrors TokenAction): AddLiquidity uses
/// amount_hkm/amount_token/min_shares; RemoveLiquidity uses
/// shares/min_hkm/min_token; Swap uses amount_in/hkm_to_token/min_out. Every
/// pool pairs an HTS token with HKM, so `token_id` identifies the pool.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct AmmAction {
    pub token_id: String,
    #[serde(default)]
    pub amount_hkm: u64,
    #[serde(default)]
    pub amount_token: u64,
    #[serde(default)]
    pub shares: u64,
    #[serde(default)]
    pub amount_in: u64,
    #[serde(default)]
    pub hkm_to_token: bool,
    #[serde(default)]
    pub min_shares: u64,
    #[serde(default)]
    pub min_hkm: u64,
    #[serde(default)]
    pub min_token: u64,
    #[serde(default)]
    pub min_out: u64,
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
    /// Native-token action (TokenCreate / TokenTransfer / TokenBurn only).
    #[serde(default)]
    pub token: Option<TokenAction>,
    /// AMM action (AddLiquidity / RemoveLiquidity / Swap only).
    #[serde(default)]
    pub amm: Option<AmmAction>,
    /// ML-DSA-65 public key (hex), for hybrid accounts only.
    ///
    /// A hybrid account's address commits to BOTH this and `public_key`, so
    /// neither can be swapped: substituting either produces a different
    /// address, which is a different account.
    #[serde(default)]
    pub pq_public_key: Option<String>,
    /// ML-DSA-65 signature (hex) over the same message as `signature`.
    ///
    /// Both must verify, so forging requires breaking both schemes.
    #[serde(default)]
    pub pq_signature: Option<String>,
    /// Which network this transaction is for.
    ///
    /// Without it, a signature is valid on every Hikmalayer network at once:
    /// addresses are derived from the key, so a user has the same address on
    /// a testnet and on mainnet, and a transaction they signed while testing
    /// replays verbatim against their real balance. The chain id is part of
    /// the signed message and is checked against the chain's own id on
    /// apply, so a signature made for one network is inert on every other.
    #[serde(default)]
    pub chain_id: String,
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
            token: None,
            amm: None,
            pq_public_key: None,
            pq_signature: None,
            chain_id: String::new(),
        }
    }

    /// Set the network this transaction is for. Chainable, so a client can
    /// write `Transaction::new(..).for_chain(&chain_id)`.
    pub fn for_chain(mut self, chain_id: &str) -> Self {
        self.chain_id = chain_id.to_string();
        self
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

    /// Canonical message a creator signs to issue a new native token.
    /// Binds every immutable parameter and the initial supply.
    pub fn token_create_signing_message(
        symbol: &str,
        name: &str,
        decimals: u32,
        initial_supply: u64,
        nonce: u64,
    ) -> String {
        format!(
            "hikmalayer-token-create:{}:{}:{}:{}:{}",
            symbol, name, decimals, initial_supply, nonce
        )
    }

    /// Canonical message a holder signs to transfer native-token units.
    pub fn token_transfer_signing_message(
        token_id: &str,
        to: &str,
        amount: u64,
        nonce: u64,
    ) -> String {
        format!(
            "hikmalayer-token-transfer:{}:{}:{}:{}",
            token_id, to, amount, nonce
        )
    }

    /// Canonical message a holder signs to burn native-token units.
    pub fn token_burn_signing_message(token_id: &str, amount: u64, nonce: u64) -> String {
        format!("hikmalayer-token-burn:{}:{}:{}", token_id, amount, nonce)
    }

    /// Canonical message signed to add liquidity to a pool.
    pub fn amm_add_signing_message(
        token_id: &str,
        amount_hkm: u64,
        amount_token: u64,
        min_shares: u64,
        nonce: u64,
    ) -> String {
        format!(
            "hikmalayer-amm-add:{}:{}:{}:{}:{}",
            token_id, amount_hkm, amount_token, min_shares, nonce
        )
    }

    /// Canonical message signed to remove liquidity from a pool.
    pub fn amm_remove_signing_message(
        token_id: &str,
        shares: u64,
        min_hkm: u64,
        min_token: u64,
        nonce: u64,
    ) -> String {
        format!(
            "hikmalayer-amm-remove:{}:{}:{}:{}:{}",
            token_id, shares, min_hkm, min_token, nonce
        )
    }

    /// Canonical message signed to swap against a pool.
    pub fn amm_swap_signing_message(
        token_id: &str,
        hkm_to_token: bool,
        amount_in: u64,
        min_out: u64,
        nonce: u64,
    ) -> String {
        format!(
            "hikmalayer-amm-swap:{}:{}:{}:{}:{}",
            token_id, hkm_to_token, amount_in, min_out, nonce
        )
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
    /// Verify a transaction's authorization without block context.
    ///
    /// `verify_for_block` needs to know which validator produced the block in
    /// order to check a `Reward`. Everything a *user* submits is authorized
    /// entirely by its own signature, so it can be checked anywhere — and
    /// `ChainState::apply_transaction` does exactly that, so applying a
    /// transaction is safe regardless of what the caller remembered to do
    /// first. Relying on call order to keep forged transactions out of the
    /// ledger is the kind of assumption that holds until one code path
    /// forgets.
    ///
    /// `Reward` and `Slash` carry no sender signature: their rules are block
    /// rules, enforced by `verify_for_block` during block validation, which
    /// is the only place they can be checked at all.
    pub fn verify_authorization(&self) -> Result<(), String> {
        match self.transaction_type {
            TransactionType::Reward | TransactionType::Slash => Ok(()),
            _ => self.verify_for_block(""),
        }
    }

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
            TransactionType::TokenCreate => {
                let from = self
                    .from
                    .as_ref()
                    .ok_or_else(|| "TokenCreate missing creator".to_string())?;
                let action = self
                    .token
                    .as_ref()
                    .ok_or_else(|| "TokenCreate missing token action".to_string())?;
                let symbol = action.symbol.trim();
                if symbol.is_empty() || symbol.len() > MAX_TOKEN_SYMBOL_LEN {
                    return Err(format!(
                        "Token symbol must be 1..={} characters",
                        MAX_TOKEN_SYMBOL_LEN
                    ));
                }
                if action.name.len() > MAX_TOKEN_NAME_LEN {
                    return Err(format!(
                        "Token name must be at most {} characters",
                        MAX_TOKEN_NAME_LEN
                    ));
                }
                if action.decimals > MAX_TOKEN_DECIMALS {
                    return Err(format!(
                        "Token decimals must be at most {}",
                        MAX_TOKEN_DECIMALS
                    ));
                }
                if self.amount == 0 {
                    return Err("Token initial supply must be greater than zero".to_string());
                }
                let message = Self::token_create_signing_message(
                    &action.symbol,
                    &action.name,
                    action.decimals,
                    self.amount,
                    self.nonce,
                );
                self.verify_sender_signature(from, &message)
            }
            TransactionType::TokenTransfer => {
                let from = self
                    .from
                    .as_ref()
                    .ok_or_else(|| "TokenTransfer missing sender".to_string())?;
                let action = self
                    .token
                    .as_ref()
                    .ok_or_else(|| "TokenTransfer missing token action".to_string())?;
                if action.token_id.trim().is_empty() {
                    return Err("TokenTransfer missing token id".to_string());
                }
                if self.to.trim().is_empty() {
                    return Err("TokenTransfer missing recipient".to_string());
                }
                if self.amount == 0 {
                    return Err("TokenTransfer amount must be greater than zero".to_string());
                }
                let message = Self::token_transfer_signing_message(
                    &action.token_id,
                    &self.to,
                    self.amount,
                    self.nonce,
                );
                self.verify_sender_signature(from, &message)
            }
            TransactionType::TokenBurn => {
                let from = self
                    .from
                    .as_ref()
                    .ok_or_else(|| "TokenBurn missing sender".to_string())?;
                let action = self
                    .token
                    .as_ref()
                    .ok_or_else(|| "TokenBurn missing token action".to_string())?;
                if action.token_id.trim().is_empty() {
                    return Err("TokenBurn missing token id".to_string());
                }
                if self.amount == 0 {
                    return Err("TokenBurn amount must be greater than zero".to_string());
                }
                let message =
                    Self::token_burn_signing_message(&action.token_id, self.amount, self.nonce);
                self.verify_sender_signature(from, &message)
            }
            TransactionType::AddLiquidity => {
                let from = self
                    .from
                    .as_ref()
                    .ok_or_else(|| "AddLiquidity missing sender".to_string())?;
                let action = self
                    .amm
                    .as_ref()
                    .ok_or_else(|| "AddLiquidity missing amm action".to_string())?;
                if action.token_id.trim().is_empty() {
                    return Err("AddLiquidity missing token id".to_string());
                }
                if action.amount_hkm == 0 || action.amount_token == 0 {
                    return Err("AddLiquidity requires non-zero HKM and token amounts".to_string());
                }
                let message = Self::amm_add_signing_message(
                    &action.token_id,
                    action.amount_hkm,
                    action.amount_token,
                    action.min_shares,
                    self.nonce,
                );
                self.verify_sender_signature(from, &message)
            }
            TransactionType::RemoveLiquidity => {
                let from = self
                    .from
                    .as_ref()
                    .ok_or_else(|| "RemoveLiquidity missing sender".to_string())?;
                let action = self
                    .amm
                    .as_ref()
                    .ok_or_else(|| "RemoveLiquidity missing amm action".to_string())?;
                if action.token_id.trim().is_empty() {
                    return Err("RemoveLiquidity missing token id".to_string());
                }
                if action.shares == 0 {
                    return Err("RemoveLiquidity requires a non-zero share amount".to_string());
                }
                let message = Self::amm_remove_signing_message(
                    &action.token_id,
                    action.shares,
                    action.min_hkm,
                    action.min_token,
                    self.nonce,
                );
                self.verify_sender_signature(from, &message)
            }
            TransactionType::Swap => {
                let from = self
                    .from
                    .as_ref()
                    .ok_or_else(|| "Swap missing sender".to_string())?;
                let action = self
                    .amm
                    .as_ref()
                    .ok_or_else(|| "Swap missing amm action".to_string())?;
                if action.token_id.trim().is_empty() {
                    return Err("Swap missing token id".to_string());
                }
                if action.amount_in == 0 {
                    return Err("Swap requires a non-zero input amount".to_string());
                }
                let message = Self::amm_swap_signing_message(
                    &action.token_id,
                    action.hkm_to_token,
                    action.amount_in,
                    action.min_out,
                    self.nonce,
                );
                self.verify_sender_signature(from, &message)
            }
        }
    }

    /// Verify a native Hikmalayer signature: the embedded public key must
    /// derive to the sender's address and the compact secp256k1 signature
    /// must verify over the domain-prefixed message.
    /// Bind a canonical message to a network.
    ///
    /// Clients build the same string: `<chain_id>:<canonical message>`. It is
    /// deliberately a visible prefix rather than a hidden change to the
    /// digest, so a wallet's confirmation screen shows the user which network
    /// they are authorizing — "hikmalayer-mainnet:hikmalayer-transfer:…".
    pub fn scoped_signing_message(chain_id: &str, message: &str) -> String {
        format!("{}:{}", chain_id, message)
    }

    fn verify_sender_signature(&self, from: &str, message: &str) -> Result<(), String> {
        let message = &Self::scoped_signing_message(&self.chain_id, message);
        let public_key = self
            .public_key
            .as_ref()
            .ok_or_else(|| "Transaction missing public key".to_string())?;
        let signature = self
            .signature
            .as_ref()
            .ok_or_else(|| "Transaction missing signature".to_string())?;

        // The ADDRESS decides which scheme authorizes it. Reading the scheme
        // off the transaction instead would let an attacker downgrade a
        // hybrid account by simply omitting the post-quantum half.
        match hybrid::scheme_of(from) {
            Some(hybrid::AccountScheme::Hybrid) => {
                let pq_public_key = self.pq_public_key.as_ref().ok_or_else(|| {
                    "Hybrid account requires a post-quantum public key".to_string()
                })?;
                let pq_signature = self.pq_signature.as_ref().ok_or_else(|| {
                    "Hybrid account requires a post-quantum signature".to_string()
                })?;
                hybrid::verify_hybrid(
                    from,
                    message,
                    public_key,
                    pq_public_key,
                    signature,
                    pq_signature,
                )
            }
            Some(hybrid::AccountScheme::Classical) => {
                // A classical transaction carrying post-quantum material is
                // refused rather than ignored: the extra fields are outside
                // what the signature covers, so accepting them would give one
                // authorized transaction more than one valid encoding.
                if self.pq_public_key.is_some() || self.pq_signature.is_some() {
                    return Err(
                        "Classical account must not carry post-quantum fields".to_string()
                    );
                }
                let derived = pos::derive_address(public_key)?;
                if derived != *from {
                    return Err("Sender address does not match the signing key".to_string());
                }
                if !pos::verify_message(message, public_key, signature) {
                    return Err("Transaction signature verification failed".to_string());
                }
                Ok(())
            }
            None => Err(format!("Sender '{}' is not a valid Hikmalayer address", from)),
        }
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
        // the perpetual security budget. 3,700 >> 7 = 28 HKM < 50 HKM, so
        // epoch 8 (halvings = 7) is the first tail epoch.
        assert!(BLOCK_REWARD >> 7 < TAIL_EMISSION);
        assert!(BLOCK_REWARD >> 6 > TAIL_EMISSION);
        assert_eq!(block_reward(7 * HALVING_INTERVAL + 1), TAIL_EMISSION);
        assert_eq!(block_reward(100 * HALVING_INTERVAL + 1), TAIL_EMISSION);
        assert_eq!(block_reward(u64::MAX), TAIL_EMISSION);

        // Halving-phase emission + the 30B genesis allocation lands just
        // under the 100B cap (30/70 premine/mined): sum over epochs of
        // interval * reward, in whole HKM.
        let mut mined_hkm: u128 = 0;
        for epoch in 0..7u32 {
            let reward = (BLOCK_REWARD >> epoch).max(TAIL_EMISSION);
            mined_hkm += (HALVING_INTERVAL as u128) * (reward as u128)
                / (UNITS_PER_HKM as u128);
        }
        let genesis_hkm: u128 = 30_000_000_000;
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
tx.chain_id = crate::blockchain::state::DEFAULT_CHAIN_ID.to_string();
                tx.signature = Some(pos::sign_message(
            &Transaction::scoped_signing_message(
                crate::blockchain::state::DEFAULT_CHAIN_ID,
                &message,
            ),
            &private_key,
        ).unwrap());
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
tx.chain_id = crate::blockchain::state::DEFAULT_CHAIN_ID.to_string();
                tx.signature = Some(pos::sign_message(
            &Transaction::scoped_signing_message(
                crate::blockchain::state::DEFAULT_CHAIN_ID,
                &message,
            ),
            &private_key,
        ).unwrap());
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
tx.chain_id = crate::blockchain::state::DEFAULT_CHAIN_ID.to_string();
                tx.signature = Some(pos::sign_message(
            &Transaction::scoped_signing_message(
                crate::blockchain::state::DEFAULT_CHAIN_ID,
                &message,
            ),
            &private_key,
        ).unwrap());
        assert!(tx.verify_for_block("validator-1").is_ok());

        // Wrong destination account: rejected.
        tx.to = "hkmsomewhere".to_string();
        assert!(tx.verify_for_block("validator-1").is_err());
    }
}
