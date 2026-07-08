use super::block::Block;
use super::state::ChainState;
use super::transaction::{Transaction, TransactionType};
use crate::consensus::{pos, pow, vrf};
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};

/// Maximum tolerated clock skew (seconds) for incoming block timestamps.
pub const MAX_TIMESTAMP_SKEW_SECONDS: i64 = 120;

/// Default initial supply for networks that do not configure one.
pub const DEFAULT_GENESIS_SUPPLY: u64 = 1_000_000;

/// Well-known development key (32-byte big-endian 1). Used as the default
/// genesis treasury/validator on LOCAL networks only — real networks must
/// configure GENESIS_TREASURY_ADDRESS / GENESIS_VALIDATOR_PUBLIC_KEY.
pub fn dev_genesis_private_key() -> String {
    let mut bytes = [0u8; 32];
    bytes[31] = 1;
    hex::encode(bytes)
}

fn default_genesis_validator_public_key() -> Option<String> {
    pos::derive_public_key(&dev_genesis_private_key()).ok()
}

fn default_genesis_validator_vrf_public_key() -> Option<String> {
    vrf::derive_vrf_public_key(&dev_genesis_private_key()).ok()
}

fn default_genesis_treasury() -> String {
    default_genesis_validator_public_key()
        .and_then(|key| pos::derive_address(&key).ok())
        .unwrap_or_default()
}

fn default_genesis_supply() -> u64 {
    DEFAULT_GENESIS_SUPPLY
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Blockchain {
    pub blocks: Vec<Block>,
    pub difficulty: usize,
    #[serde(default)]
    pub finalized_height: u64,
    /// Genesis network parameters — part of chain identity. Two nodes with
    /// different parameters produce different genesis hashes and never sync.
    #[serde(default = "default_genesis_treasury")]
    pub genesis_treasury: String,
    #[serde(default = "default_genesis_validator_public_key")]
    pub genesis_validator_public_key: Option<String>,
    #[serde(default = "default_genesis_validator_vrf_public_key")]
    pub genesis_validator_vrf_public_key: Option<String>,
    #[serde(default = "default_genesis_supply")]
    pub genesis_supply: u64,
    /// Current chain state: derived from the blocks, never persisted.
    /// Rebuilt via `rebuild_state` after deserialization.
    #[serde(skip)]
    pub state: ChainState,
    /// Randomness beacon at the current tip: a fold of every block's VRF
    /// output, seeding unbiasable leader election. Derived, never persisted.
    #[serde(skip)]
    pub randomness: String,
}

/// A validation failure. `slashable` distinguishes provable validator
/// misbehavior (wrong slot, bad signature, bad PoW, tampered payload or
/// state) from structural problems (missing fields, stale tip).
#[derive(Debug, Clone)]
pub struct BlockError {
    pub reason: String,
    pub slashable: bool,
}

impl BlockError {
    fn structural(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
            slashable: false,
        }
    }

    fn slashable(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
            slashable: true,
        }
    }
}

impl Blockchain {
    /// New chain with default (development) genesis parameters.
    pub fn new(difficulty: usize) -> Self {
        Self::new_with_genesis(
            difficulty,
            default_genesis_treasury(),
            default_genesis_validator_public_key(),
            default_genesis_validator_vrf_public_key(),
            default_genesis_supply(),
        )
    }

    pub fn new_with_genesis(
        difficulty: usize,
        genesis_treasury: String,
        genesis_validator_public_key: Option<String>,
        genesis_validator_vrf_public_key: Option<String>,
        genesis_supply: u64,
    ) -> Self {
        let difficulty = pow::clamp_difficulty(difficulty);
        let state = ChainState::genesis(
            &genesis_treasury,
            genesis_validator_public_key.as_deref(),
            genesis_validator_vrf_public_key.as_deref(),
            genesis_supply,
        );
        let genesis_block = Block::genesis(difficulty, state.state_root());
        // The beacon starts at the (deterministic) genesis hash.
        let randomness = genesis_block.hash.clone();
        Blockchain {
            blocks: vec![genesis_block],
            difficulty,
            finalized_height: 0,
            genesis_treasury,
            genesis_validator_public_key,
            genesis_validator_vrf_public_key,
            genesis_supply,
            state,
            randomness,
        }
    }

    fn genesis_state(&self) -> ChainState {
        ChainState::genesis(
            &self.genesis_treasury,
            self.genesis_validator_public_key.as_deref(),
            self.genesis_validator_vrf_public_key.as_deref(),
            self.genesis_supply,
        )
    }

    pub fn latest_hash(&self) -> String {
        self.blocks
            .last()
            .map(|block| block.hash.clone())
            .unwrap_or_else(|| "0".to_string())
    }

    pub fn create_block(
        &self,
        transactions: Vec<String>,
        validator: Option<String>,
        validator_public_key: Option<String>,
        state_root: String,
    ) -> Block {
        let index = self.blocks.len() as u64;
        let previous_hash = self.latest_hash();
        Block::new(
            index,
            transactions,
            previous_hash,
            self.difficulty,
            validator,
            validator_public_key,
            None,
            state_root,
        )
    }

    /// Append a fully validated block, adopt its post-state, and fold its
    /// VRF output into the randomness beacon.
    pub fn commit_block(&mut self, block: Block, post_state: ChainState) {
        self.randomness = vrf::next_randomness(
            &self.randomness,
            block.vrf_output.as_deref().unwrap_or_default(),
        );
        self.blocks.push(block);
        self.state = post_state;
    }

    pub fn apply_finality(&mut self, finality_depth: u64) {
        if self.blocks.is_empty() {
            self.finalized_height = 0;
            return;
        }
        if finality_depth == 0 {
            self.finalized_height = self.blocks.len() as u64 - 1;
            return;
        }
        let chain_len = self.blocks.len() as u64;
        if chain_len > finality_depth {
            self.finalized_height = chain_len - 1 - finality_depth;
        } else {
            self.finalized_height = 0;
        }
    }

    /// Total expected PoW work across the chain — the fork-choice weight.
    pub fn cumulative_work(&self) -> u128 {
        self.blocks
            .iter()
            .map(|block| pow::work_for_difficulty(block.difficulty))
            .sum()
    }

    /// Full consensus validation of a single non-genesis block against its
    /// expected position, the state at its parent, and the randomness beacon
    /// at its parent. On success returns the post-execution state.
    fn validate_block_at(
        block: &Block,
        expected_index: u64,
        expected_previous_hash: &str,
        parent_state: &ChainState,
        parent_randomness: &str,
    ) -> Result<ChainState, BlockError> {
        if block.index != expected_index {
            return Err(BlockError::structural(
                "Block index does not match chain tip",
            ));
        }

        if block.previous_hash != expected_previous_hash {
            return Err(BlockError::structural(
                "Block previous hash does not match chain tip",
            ));
        }

        if !pow::is_difficulty_in_bounds(block.difficulty) {
            return Err(BlockError::slashable("Block difficulty out of bounds"));
        }

        if block.validator.is_none()
            || block.validator_public_key.is_none()
            || block.validator_signature.is_none()
        {
            return Err(BlockError::structural(
                "Block missing required validator data",
            ));
        }

        // PoS: the validator set is the ON-CHAIN set at the parent state,
        // and the slot seed comes from the VRF randomness beacon — the
        // producer of the parent block could not grind it.
        let validator_set = parent_state.validator_set();
        let slot_input = vrf::slot_input(parent_randomness, block.index);
        let expected_validator = pos::select_staker_with_seed(&slot_input, &validator_set);
        if expected_validator.is_none() {
            return Err(BlockError::structural(
                "No validators registered at parent state",
            ));
        }
        if expected_validator != block.validator {
            return Err(BlockError::slashable(
                "Block validator does not match PoS selection",
            ));
        }

        // The block must be signed with the validator's on-chain key.
        let validator = block.validator.as_ref().unwrap();
        let public_key = block.validator_public_key.as_ref().unwrap();
        let registered_key = parent_state
            .stakers
            .get(validator)
            .map(|info| info.public_key.as_str());
        if registered_key != Some(public_key.as_str()) {
            return Err(BlockError::slashable(
                "Block signer key does not match validator's registered key",
            ));
        }

        let signature = block.validator_signature.as_ref().unwrap();
        if !pos::verify_block_signature(&block.hash, public_key, signature) {
            return Err(BlockError::slashable("Block signature verification failed"));
        }

        // VRF: the block must carry the unique, verifiable randomness
        // contribution of its validator for this exact slot.
        let registered_vrf_key = parent_state
            .stakers
            .get(validator)
            .map(|info| info.vrf_public_key.as_str())
            .unwrap_or_default();
        if registered_vrf_key.is_empty() {
            return Err(BlockError::structural(
                "Validator has no registered VRF key",
            ));
        }
        let (Some(vrf_output), Some(vrf_proof)) = (&block.vrf_output, &block.vrf_proof) else {
            return Err(BlockError::structural(
                "Block missing VRF output or proof",
            ));
        };
        if !vrf::verify(&slot_input, registered_vrf_key, vrf_output, vrf_proof) {
            return Err(BlockError::slashable("Block VRF verification failed"));
        }

        if !block.has_valid_merkle_root() {
            return Err(BlockError::slashable("Block merkle root mismatch"));
        }

        if !block.has_valid_pow() {
            return Err(BlockError::slashable("Block PoW validation failed"));
        }

        // Execute the block: every transaction must be statelessly valid,
        // apply cleanly to the parent state, and the block must pay exactly
        // one correct reward.
        let mut post_state = parent_state.clone();
        let mut reward_count = 0usize;
        for tx_str in &block.transactions {
            let tx: Transaction = serde_json::from_str(tx_str)
                .map_err(|_| BlockError::slashable("Block contains malformed transaction"))?;
            tx.verify_for_block(validator)
                .map_err(BlockError::slashable)?;
            post_state
                .apply_transaction(&tx)
                .map_err(BlockError::slashable)?;
            if tx.transaction_type == TransactionType::Reward {
                reward_count += 1;
            }
        }
        if reward_count != 1 {
            return Err(BlockError::slashable(
                "Block must contain exactly one reward transaction",
            ));
        }

        // The block's committed state root must match actual execution.
        if post_state.state_root() != block.state_root {
            return Err(BlockError::slashable(
                "Block state root does not match executed state",
            ));
        }

        Ok(post_state)
    }

    /// Validate a candidate block for appending to the current tip. Adds
    /// tip-difficulty and timestamp-freshness rules on top of the consensus
    /// checks. On success returns the post-execution state to commit.
    pub fn validate_block_candidate(&self, block: &Block) -> Result<ChainState, String> {
        if block.difficulty != self.difficulty {
            return Err("Block difficulty does not match chain difficulty".to_string());
        }

        let now = Utc::now();
        if block.timestamp > now + Duration::seconds(MAX_TIMESTAMP_SKEW_SECONDS) {
            return Err("Block timestamp is too far in the future".to_string());
        }

        Self::validate_block_at(
            block,
            self.blocks.len() as u64,
            &self.latest_hash(),
            &self.state,
            &self.randomness,
        )
        .map_err(|err| err.reason)
    }

    /// The VRF input for the next slot at the current tip. Exposed so the
    /// selected validator can compute its VRF proof offline.
    pub fn next_slot_input(&self) -> String {
        vrf::slot_input(&self.randomness, self.blocks.len() as u64)
    }

    /// Cheap integrity probe for hot read endpoints: verifies the tip
    /// commitment against the live state without replaying the chain.
    /// Full replay validation remains available via `validate_full`.
    pub fn quick_integrity(&self) -> bool {
        match self.blocks.last() {
            Some(tip) => {
                tip.state_root == self.state.state_root()
                    && tip.has_valid_pow()
                    && (self.blocks.len() < 2
                        || tip.previous_hash == self.blocks[self.blocks.len() - 2].hash)
            }
            None => false,
        }
    }

    /// Validate the entire chain from genesis by replaying the state
    /// machine and the randomness beacon. Returns the final (state,
    /// randomness), or the first failure.
    pub fn validate_full(&self) -> Result<(ChainState, String), (usize, BlockError)> {
        let genesis = self
            .blocks
            .first()
            .ok_or((0, BlockError::structural("Chain is empty")))?;
        if !genesis.has_valid_pow() || !genesis.has_valid_merkle_root() {
            return Err((0, BlockError::structural("Genesis block failed validation")));
        }

        let mut state = self.genesis_state();
        if state.state_root() != genesis.state_root {
            return Err((
                0,
                BlockError::structural("Genesis state root does not match network parameters"),
            ));
        }

        let mut randomness = genesis.hash.clone();
        for i in 1..self.blocks.len() {
            let current = &self.blocks[i];
            let previous = &self.blocks[i - 1];
            state =
                Self::validate_block_at(current, i as u64, &previous.hash, &state, &randomness)
                    .map_err(|error| (i, error))?;
            randomness = vrf::next_randomness(
                &randomness,
                current.vrf_output.as_deref().unwrap_or_default(),
            );
        }
        Ok((state, randomness))
    }

    pub fn is_valid(&self) -> bool {
        self.validate_full().is_ok()
    }

    /// Recompute `state` from the block history (after deserialization or
    /// adoption). Fails if the stored chain does not validate.
    pub fn rebuild_state(&mut self) -> Result<(), String> {
        match self.validate_full() {
            Ok((state, randomness)) => {
                self.state = state;
                self.randomness = randomness;
                Ok(())
            }
            Err((index, error)) => Err(format!("Block {}: {}", index, error.reason)),
        }
    }

    /// Report chain validity with details (no side effects — slashing is an
    /// on-chain transaction, not a local mutation).
    pub fn validate_report(&self) -> (bool, Option<String>) {
        match self.validate_full() {
            Ok(_) => (true, None),
            Err((index, error)) => (false, Some(format!("Block {}: {}", index, error.reason))),
        }
    }

    pub fn evaluate_slash_evidence(&self, block_index: u64) -> Result<SlashEvidence, String> {
        let index = block_index as usize;
        if index == 0 {
            return Err("Cannot slash genesis block".to_string());
        }
        if index >= self.blocks.len() {
            return Err("Block index out of range".to_string());
        }

        match self.validate_full() {
            Ok(_) => Err("Block does not contain slashable behavior".to_string()),
            Err((failed_index, error)) if failed_index == index && error.slashable => {
                Ok(SlashEvidence {
                    validator: self.blocks[index]
                        .validator
                        .clone()
                        .unwrap_or_else(|| "unknown".to_string()),
                    reason: error.reason,
                    timestamp: Utc::now().to_rfc3339(),
                })
            }
            Err(_) => Err("Block does not contain slashable behavior".to_string()),
        }
    }

    /// Fork choice: adopt `candidate` if it shares our genesis, replays
    /// cleanly under OUR network parameters, preserves every finalized
    /// block, and carries strictly more cumulative work.
    pub fn try_adopt_chain(&mut self, candidate: &Blockchain) -> Result<bool, String> {
        let our_genesis = self
            .blocks
            .first()
            .ok_or_else(|| "Local chain is empty".to_string())?;
        let their_genesis = candidate
            .blocks
            .first()
            .ok_or_else(|| "Candidate chain is empty".to_string())?;

        if our_genesis.hash != their_genesis.hash {
            return Err("Candidate chain has a different genesis block".to_string());
        }

        // Finalized blocks are irreversible: the candidate must contain the
        // identical prefix up to our finalized height.
        for height in 0..=self.finalized_height as usize {
            let local = &self.blocks[height];
            let remote = candidate
                .blocks
                .get(height)
                .ok_or_else(|| "Candidate chain rewrites finalized history".to_string())?;
            if local.hash != remote.hash {
                return Err("Candidate chain rewrites finalized history".to_string());
            }
        }

        if candidate.cumulative_work() <= self.cumulative_work() {
            return Ok(false);
        }

        // Never trust the candidate's own genesis parameters: replay its
        // blocks under our network configuration.
        let mut replay = Blockchain {
            blocks: candidate.blocks.clone(),
            difficulty: self.difficulty,
            finalized_height: 0,
            genesis_treasury: self.genesis_treasury.clone(),
            genesis_validator_public_key: self.genesis_validator_public_key.clone(),
            genesis_validator_vrf_public_key: self.genesis_validator_vrf_public_key.clone(),
            genesis_supply: self.genesis_supply,
            state: ChainState::default(),
            randomness: String::new(),
        };
        replay
            .rebuild_state()
            .map_err(|err| format!("Candidate chain failed replay: {}", err))?;

        self.blocks = replay.blocks;
        self.state = replay.state;
        self.randomness = replay.randomness;
        Ok(true)
    }
}

impl Default for Blockchain {
    fn default() -> Self {
        Blockchain::new(2)
    }
}

pub struct SlashEvidence {
    pub validator: String,
    pub reason: String,
    pub timestamp: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blockchain::state::STAKING_POOL_ACCOUNT;
    use crate::blockchain::transaction::SlashProof;

    /// The dev genesis validator (treasury) — key 0x…01.
    fn treasury() -> (String, String, String) {
        let private_key = dev_genesis_private_key();
        let public_key = pos::derive_public_key(&private_key).unwrap();
        let address = pos::derive_address(&public_key).unwrap();
        (address, public_key, private_key)
    }

    fn wallet(seed: u8) -> (String, String, String) {
        let private_key = hex::encode([seed; 32]);
        let public_key = pos::derive_public_key(&private_key).unwrap();
        let address = pos::derive_address(&public_key).unwrap();
        (address, public_key, private_key)
    }

    /// Sign a block for whichever validator PoS selected, using the provided
    /// keyring of (address, private_key) pairs.
    fn keyring() -> Vec<(String, String)> {
        let (t_addr, _, t_key) = treasury();
        let mut ring = vec![(t_addr, t_key)];
        for seed in 2..=9u8 {
            let (addr, _, key) = wallet(seed);
            ring.push((addr, key));
        }
        ring
    }

    /// Build, sign, validate, and commit the next block carrying `extra`
    /// transactions (plus the mandatory reward).
    fn mine_block_with(chain: &mut Blockchain, extra: Vec<Transaction>) -> Result<(), String> {
        let validator_set = chain.state.validator_set();
        let slot_input = chain.next_slot_input();
        let validator = pos::select_staker_with_seed(&slot_input, &validator_set)
            .ok_or("no validators".to_string())?;
        let public_key = chain.state.stakers.get(&validator).unwrap().public_key.clone();

        let mut post = chain.state.clone();
        let mut txs = Vec::new();
        for tx in &extra {
            tx.verify_for_block(&validator)?;
            post.apply_transaction(tx)?;
            txs.push(serde_json::to_string(tx).unwrap());
        }
        let reward = Transaction::new_reward(&validator);
        post.apply_transaction(&reward)?;
        txs.push(serde_json::to_string(&reward).unwrap());

        let mut block = chain.create_block(
            txs,
            Some(validator.clone()),
            Some(public_key),
            post.state_root(),
        );
        let private_key = keyring()
            .into_iter()
            .find(|(addr, _)| *addr == validator)
            .map(|(_, key)| key)
            .ok_or("no key for selected validator".to_string())?;
        let (vrf_output, vrf_proof) = vrf::prove(&slot_input, &private_key)?;
        block.vrf_output = Some(vrf_output);
        block.vrf_proof = Some(vrf_proof);
        block.validator_signature = Some(pos::sign_block_hash(&block.hash, &private_key).unwrap());

        let post_state = chain.validate_block_candidate(&block)?;
        chain.commit_block(block, post_state);
        Ok(())
    }

    fn mine_valid_block(chain: &mut Blockchain) {
        mine_block_with(chain, Vec::new()).expect("block should be valid");
    }

    fn signed_transfer(
        from: (&str, &str, &str), // address, public_key, private_key
        to: &str,
        amount: u64,
        nonce: u64,
    ) -> Transaction {
        let mut tx = Transaction::new(
            Some(from.0.to_string()),
            to.to_string(),
            amount,
            TransactionType::Transfer,
        );
        tx.nonce = nonce;
        tx.public_key = Some(from.1.to_string());
        let message = Transaction::transfer_signing_message(from.0, to, amount, nonce);
        tx.signature = Some(pos::sign_message(&message, from.2).unwrap());
        tx
    }

    #[test]
    fn chain_grows_and_replays_deterministically() {
        let mut chain = Blockchain::default();
        assert_eq!(chain.blocks.len(), 1);
        mine_valid_block(&mut chain);
        mine_valid_block(&mut chain);
        assert_eq!(chain.blocks.len(), 3);
        assert!(chain.is_valid());

        // Rebuilding state from scratch converges to the same root.
        let runtime_root = chain.state.state_root();
        chain.rebuild_state().unwrap();
        assert_eq!(chain.state.state_root(), runtime_root);
    }

    #[test]
    fn transfers_execute_on_chain() {
        let mut chain = Blockchain::default();
        let (t_addr, t_pub, t_key) = treasury();
        let (dest, ..) = wallet(2);

        let tx = signed_transfer((&t_addr, &t_pub, &t_key), &dest, 500, 1);
        mine_block_with(&mut chain, vec![tx]).unwrap();

        assert_eq!(chain.state.balance_of(&dest), 500);
        assert_eq!(chain.state.nonce_of(&t_addr), 1);
        assert!(chain.is_valid());
    }

    #[test]
    fn on_chain_staking_admits_new_validator() {
        let mut chain = Blockchain::default();
        let (t_addr, t_pub, t_key) = treasury();
        let (v_addr, v_pub, v_key) = wallet(2);

        // Fund the new validator, then stake on-chain.
        let fund = signed_transfer((&t_addr, &t_pub, &t_key), &v_addr, 500, 1);
        mine_block_with(&mut chain, vec![fund]).unwrap();

        let mut stake = Transaction::new(
            Some(v_addr.clone()),
            STAKING_POOL_ACCOUNT.to_string(),
            300,
            TransactionType::Stake,
        );
        stake.nonce = 1;
        stake.public_key = Some(v_pub.clone());
        let v_vrf = vrf::derive_vrf_public_key(&v_key).unwrap();
        stake.vrf_public_key = Some(v_vrf.clone());
        let message = Transaction::stake_signing_message(&v_addr, 300, 1, &v_vrf);
        stake.signature = Some(pos::sign_message(&message, &v_key).unwrap());
        mine_block_with(&mut chain, vec![stake]).unwrap();

        assert_eq!(chain.state.validator_set().len(), 2);
        assert!(chain.is_valid());

        // The new validator can now be selected and produce blocks.
        for _ in 0..4 {
            mine_valid_block(&mut chain);
        }
        assert!(chain.is_valid());
    }

    #[test]
    fn rejects_forged_state_root() {
        let chain = Blockchain::default();
        let (t_addr, t_pub, t_key) = treasury();

        let reward = Transaction::new_reward(&t_addr);
        let mut post = chain.state.clone();
        post.apply_transaction(&reward).unwrap();
        // Forge extra balance into the state the block claims.
        post.balances.insert("hkmthief".to_string(), 1_000_000);

        let mut block = chain.create_block(
            vec![serde_json::to_string(&reward).unwrap()],
            Some(t_addr),
            Some(t_pub),
            post.state_root(),
        );
        let (vrf_output, vrf_proof) =
            vrf::prove(&chain.next_slot_input(), &t_key).unwrap();
        block.vrf_output = Some(vrf_output);
        block.vrf_proof = Some(vrf_proof);
        block.validator_signature = Some(pos::sign_block_hash(&block.hash, &t_key).unwrap());

        let error = chain.validate_block_candidate(&block).unwrap_err();
        assert!(error.contains("state root"), "unexpected error: {error}");
    }

    #[test]
    fn rejects_unsigned_candidate_and_wrong_key() {
        let chain = Blockchain::default();
        let (t_addr, t_pub, _) = treasury();
        let reward = Transaction::new_reward(&t_addr);
        let mut post = chain.state.clone();
        post.apply_transaction(&reward).unwrap();

        let mut block = chain.create_block(
            vec![serde_json::to_string(&reward).unwrap()],
            Some(t_addr.clone()),
            Some(t_pub),
            post.state_root(),
        );
        assert!(chain.validate_block_candidate(&block).is_err());

        // Signature from a non-registered key fails.
        let (_, _, intruder) = wallet(9);
        block.validator_signature = Some(pos::sign_block_hash(&block.hash, &intruder).unwrap());
        assert!(chain.validate_block_candidate(&block).is_err());
    }

    #[test]
    fn detects_tampered_transactions() {
        let mut chain = Blockchain::default();
        mine_valid_block(&mut chain);
        assert!(chain.is_valid());

        chain.blocks[1].transactions = vec!["Forged".to_string()];
        assert!(!chain.is_valid());
    }

    #[test]
    fn fork_choice_adopts_heavier_chain_and_rebuilds_state() {
        let mut local = Blockchain::default();
        mine_valid_block(&mut local);

        let mut remote = Blockchain::default();
        let (t_addr, t_pub, t_key) = treasury();
        let (dest, ..) = wallet(3);
        let tx = signed_transfer((&t_addr, &t_pub, &t_key), &dest, 250, 1);
        mine_block_with(&mut remote, vec![tx]).unwrap();
        mine_valid_block(&mut remote);
        mine_valid_block(&mut remote);

        let adopted = local.try_adopt_chain(&remote).unwrap();
        assert!(adopted);
        assert_eq!(local.blocks.len(), 4);
        // State was rebuilt from the adopted history.
        assert_eq!(local.state.balance_of(&dest), 250);
        assert!(local.is_valid());
    }

    #[test]
    fn fork_choice_ignores_lighter_chain() {
        let mut local = Blockchain::default();
        mine_valid_block(&mut local);
        mine_valid_block(&mut local);

        let mut remote = Blockchain::default();
        mine_valid_block(&mut remote);

        let adopted = local.try_adopt_chain(&remote).unwrap();
        assert!(!adopted);
        assert_eq!(local.blocks.len(), 3);
    }

    #[test]
    fn fork_choice_protects_finalized_history() {
        let mut local = Blockchain::default();
        mine_valid_block(&mut local);
        mine_valid_block(&mut local);
        local.apply_finality(0);

        let mut remote = Blockchain::default();
        mine_valid_block(&mut remote);
        mine_valid_block(&mut remote);
        mine_valid_block(&mut remote);

        let error = local.try_adopt_chain(&remote).unwrap_err();
        assert!(error.contains("finalized"), "unexpected error: {error}");
    }

    #[test]
    fn rejects_future_timestamps() {
        let chain = Blockchain::default();
        let (t_addr, t_pub, t_key) = treasury();
        let reward = Transaction::new_reward(&t_addr);
        let mut post = chain.state.clone();
        post.apply_transaction(&reward).unwrap();

        let mut block = chain.create_block(
            vec![serde_json::to_string(&reward).unwrap()],
            Some(t_addr),
            Some(t_pub),
            post.state_root(),
        );
        block.timestamp = Utc::now() + Duration::seconds(MAX_TIMESTAMP_SKEW_SECONDS + 60);
        block.validator_signature = Some(pos::sign_block_hash(&block.hash, &t_key).unwrap());

        let error = chain.validate_block_candidate(&block).unwrap_err();
        assert!(error.contains("future"), "unexpected error: {error}");
    }

    #[test]
    fn rejects_missing_or_forged_vrf() {
        let chain = Blockchain::default();
        let (t_addr, t_pub, t_key) = treasury();
        let reward = Transaction::new_reward(&t_addr);
        let mut post = chain.state.clone();
        post.apply_transaction(&reward).unwrap();

        let mut block = chain.create_block(
            vec![serde_json::to_string(&reward).unwrap()],
            Some(t_addr),
            Some(t_pub),
            post.state_root(),
        );
        block.validator_signature =
            Some(pos::sign_block_hash(&block.hash, &t_key).unwrap());

        // No VRF material at all.
        let error = chain.validate_block_candidate(&block).unwrap_err();
        assert!(error.contains("VRF"), "unexpected error: {error}");

        // VRF proof produced by the WRONG key.
        let intruder = hex::encode([9u8; 32]);
        let (output, proof) = vrf::prove(&chain.next_slot_input(), &intruder).unwrap();
        block.vrf_output = Some(output);
        block.vrf_proof = Some(proof);
        let error = chain.validate_block_candidate(&block).unwrap_err();
        assert!(
            error.contains("VRF verification failed"),
            "unexpected error: {error}"
        );

        // VRF proof over the WRONG slot input.
        let (output, proof) = vrf::prove("some-other-input", &t_key).unwrap();
        block.vrf_output = Some(output);
        block.vrf_proof = Some(proof);
        let error = chain.validate_block_candidate(&block).unwrap_err();
        assert!(
            error.contains("VRF verification failed"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn randomness_beacon_evolves_and_replays() {
        let mut chain = Blockchain::default();
        let genesis_randomness = chain.randomness.clone();
        mine_valid_block(&mut chain);
        let after_one = chain.randomness.clone();
        assert_ne!(genesis_randomness, after_one);
        mine_valid_block(&mut chain);
        assert_ne!(after_one, chain.randomness);

        // Replay reconstructs the exact same beacon.
        let runtime = chain.randomness.clone();
        chain.rebuild_state().unwrap();
        assert_eq!(chain.randomness, runtime);
    }

    #[test]
    fn equivocation_proof_slashes_validator_on_chain() {
        let mut chain = Blockchain::default();
        let (t_addr, t_pub, t_key) = treasury();

        // The treasury validator signs two DIFFERENT blocks at height 1.
        let reward = Transaction::new_reward(&t_addr);
        let mut post = chain.state.clone();
        post.apply_transaction(&reward).unwrap();
        let root = post.state_root();

        let make_signed = |memo: &str| {
            let mut block = chain.create_block(
                vec![
                    serde_json::to_string(&reward).unwrap(),
                    // Vary content so hashes differ (invalid tx — irrelevant,
                    // the proof only needs hash + signature).
                    memo.to_string(),
                ],
                Some(t_addr.clone()),
                Some(t_pub.clone()),
                root.clone(),
            );
            block.validator_signature =
                Some(pos::sign_block_hash(&block.hash, &t_key).unwrap());
            block
        };
        let block_a = make_signed("fork-a");
        let block_b = make_signed("fork-b");
        assert_ne!(block_a.hash, block_b.hash);

        // Build the slash transaction and mine it into the chain.
        let mut slash = Transaction::new(None, t_addr.clone(), 0, TransactionType::Slash);
        slash.slash_proof = Some(SlashProof { block_a, block_b });
        slash.verify_for_block("anyone").unwrap();

        let stake_before = chain.state.stakers.get(&t_addr).unwrap().stake;
        mine_block_with(&mut chain, vec![slash.clone()]).unwrap();
        let stake_after = chain.state.stakers.get(&t_addr).unwrap().stake;
        assert_eq!(stake_after, stake_before - stake_before / 10);
        assert!(chain.state.burned > 0);
        assert!(chain.is_valid());

        // The same offense cannot be slashed twice.
        let mut replay = slash.clone();
        replay.id = "new-id".to_string();
        assert!(mine_block_with(&mut chain, vec![replay]).is_err());
    }
}
