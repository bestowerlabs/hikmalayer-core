use super::block::Block;
use super::state::ChainState;
use super::transaction::{Transaction, TransactionType};
use crate::consensus::{pos, pow, vrf};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

/// Maximum tolerated clock skew (seconds) for incoming block timestamps.
pub const MAX_TIMESTAMP_SKEW_SECONDS: i64 = 120;

/// Default initial supply for networks that do not configure one:
/// 20,000,000,000 HKM (the 20% genesis allocation of the ~100B HKM supply;
/// the remaining ~80B is mined via the halving schedule with tail).
pub const DEFAULT_GENESIS_SUPPLY: u64 =
    20_000_000_000 * crate::blockchain::transaction::UNITS_PER_HKM;

/// Target seconds between blocks for difficulty retargeting.
pub const TARGET_BLOCK_SECONDS: i64 = 15;

/// Blocks between deterministic difficulty adjustments.
pub const RETARGET_INTERVAL: u64 = 10;

/// Liveness rotation: if the selected leader has not produced within this
/// many seconds of the parent block, the next round's leader also becomes
/// eligible. Rounds accumulate (round r opens at r × timeout), so a dead
/// validator can delay the chain by at most one timeout, never stall it.
pub const SLOT_TIMEOUT_SECONDS: i64 = 30;

/// Upper bound on fallback rounds considered per height. Bounds validation
/// work; with the whole validator set cycled through well before the cap,
/// liveness never depends on rounds beyond it.
pub const MAX_SLOT_ROUNDS: u64 = 16;

/// Deterministic difficulty adjustment: compare the average interval of the
/// last window against the target and step by at most 1, inside PoW bounds.
fn adjusted_difficulty(current: usize, window: &[Block]) -> usize {
    if window.len() < 2 {
        return current;
    }
    let span = (window[window.len() - 1].timestamp - window[0].timestamp).num_seconds();
    let average = span / (window.len() as i64 - 1);
    if average < TARGET_BLOCK_SECONDS / 2 {
        (current + 1).min(pow::MAX_DIFFICULTY)
    } else if average > TARGET_BLOCK_SECONDS * 2 {
        current.saturating_sub(1).max(pow::MIN_DIFFICULTY)
    } else {
        current
    }
}

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
    /// Genesis validator allowlist (empty = permissionless staking). Part
    /// of chain identity: baked into the genesis state root.
    #[serde(default)]
    pub genesis_validator_allowlist: Vec<String>,
    /// Current chain state: derived from the blocks, never persisted.
    /// Rebuilt via `rebuild_state` after deserialization.
    #[serde(skip)]
    pub state: ChainState,
    /// Randomness beacon at the current tip: a fold of every block's VRF
    /// output, seeding unbiasable leader election. Derived, never persisted.
    #[serde(skip)]
    pub randomness: String,
    /// Difficulty for the NEXT block: retargeted deterministically from
    /// block timestamps every RETARGET_INTERVAL blocks. Derived, never
    /// persisted (`difficulty` is the genesis/base parameter).
    #[serde(skip)]
    pub current_difficulty: usize,
    /// Absolute height of `blocks[0]`. Zero for a genesis-rooted chain; for a
    /// checkpoint-synced (pruned) chain it is the anchor's height. All height
    /// math uses `base_height + local_index`.
    #[serde(default)]
    pub base_height: u64,
    /// Trusted root for a checkpoint-synced chain. When present, `blocks[0]`
    /// is the anchor and validation starts from this state instead of genesis.
    #[serde(default)]
    pub checkpoint: Option<CheckpointRoot>,
}

/// A weak-subjectivity checkpoint: the trusted state at an anchor block, used
/// to fast-sync a node without replaying history from genesis. Anchors must
/// sit on a difficulty-retarget boundary so the schedule stays exact.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointRoot {
    /// Trusted post-execution state of the anchor block (`blocks[0]`).
    pub state: ChainState,
    /// Randomness beacon after folding the anchor block's VRF output.
    pub randomness: String,
    /// `current_difficulty` at the anchor (the difficulty for the next block).
    pub difficulty: usize,
}

/// A self-contained fast-sync bundle: the trusted checkpoint anchor plus the
/// blocks that follow it, and the genesis network parameters (chain identity).
/// A fresh node can import this and be current without replaying from genesis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointBundle {
    pub difficulty: usize,
    pub genesis_treasury: String,
    pub genesis_validator_public_key: Option<String>,
    pub genesis_validator_vrf_public_key: Option<String>,
    pub genesis_supply: u64,
    #[serde(default)]
    pub genesis_validator_allowlist: Vec<String>,
    pub anchor: Block,
    pub checkpoint: CheckpointRoot,
    pub forward_blocks: Vec<Block>,
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
        Self::new_dev_with_allowlist(difficulty, Vec::new())
    }

    /// Well-known DEV genesis parameters plus an explicit validator
    /// allowlist. Used by the node when no genesis parameters are
    /// configured but an allowlist is — the allowlist must be honored on
    /// EVERY genesis path, never silently dropped.
    pub fn new_dev_with_allowlist(difficulty: usize, allowlist: Vec<String>) -> Self {
        Self::new_with_genesis(
            difficulty,
            default_genesis_treasury(),
            default_genesis_validator_public_key(),
            default_genesis_validator_vrf_public_key(),
            default_genesis_supply(),
            allowlist,
        )
    }

    pub fn new_with_genesis(
        difficulty: usize,
        genesis_treasury: String,
        genesis_validator_public_key: Option<String>,
        genesis_validator_vrf_public_key: Option<String>,
        genesis_supply: u64,
        genesis_validator_allowlist: Vec<String>,
    ) -> Self {
        let difficulty = pow::clamp_difficulty(difficulty);
        let state = ChainState::genesis(
            &genesis_treasury,
            genesis_validator_public_key.as_deref(),
            genesis_validator_vrf_public_key.as_deref(),
            genesis_supply,
            &genesis_validator_allowlist,
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
            genesis_validator_allowlist,
            state,
            randomness,
            current_difficulty: difficulty,
            base_height: 0,
            checkpoint: None,
        }
    }

    /// Absolute index the next appended block will carry.
    pub fn next_index(&self) -> u64 {
        self.base_height + self.blocks.len() as u64
    }

    /// Absolute height of the current tip.
    pub fn tip_index(&self) -> u64 {
        self.next_index().saturating_sub(1)
    }

    /// Deterministic retarget decision shared by block production and
    /// validation: at retarget boundaries, adjust difficulty from the last
    /// RETARGET_INTERVAL blocks' timestamps (excluding genesis when rooted at
    /// it). `blocks` is the prefix ending at the just-produced block.
    fn retarget(blocks: &[Block], produced_abs: u64, base_height: u64, current: usize) -> usize {
        if (produced_abs + 1) % RETARGET_INTERVAL != 0 {
            return current;
        }
        let mut start = blocks.len().saturating_sub(RETARGET_INTERVAL as usize);
        if base_height == 0 {
            start = start.max(1); // never fold the fixed-timestamp genesis block
        }
        adjusted_difficulty(current, &blocks[start..])
    }

    /// Bootstrap a pruned chain from a trusted checkpoint bundle plus the
    /// blocks that follow. The anchor must be a retarget boundary and commit
    /// to the checkpoint state; forward blocks are validated normally.
    #[allow(clippy::too_many_arguments)]
    pub fn from_checkpoint(
        difficulty: usize,
        genesis_treasury: String,
        genesis_validator_public_key: Option<String>,
        genesis_validator_vrf_public_key: Option<String>,
        genesis_supply: u64,
        genesis_validator_allowlist: Vec<String>,
        anchor: Block,
        checkpoint: CheckpointRoot,
        forward_blocks: Vec<Block>,
    ) -> Result<Self, String> {
        if anchor.index % RETARGET_INTERVAL != 0 {
            return Err("Checkpoint anchor must sit on a retarget boundary".to_string());
        }
        if anchor.state_root != checkpoint.state.state_root() {
            return Err("Anchor state root does not match the checkpoint state".to_string());
        }
        let mut blocks = Vec::with_capacity(1 + forward_blocks.len());
        blocks.push(anchor.clone());
        blocks.extend(forward_blocks);
        let mut chain = Blockchain {
            blocks,
            difficulty: pow::clamp_difficulty(difficulty),
            finalized_height: anchor.index,
            genesis_treasury,
            genesis_validator_public_key,
            genesis_validator_vrf_public_key,
            genesis_supply,
            genesis_validator_allowlist,
            state: ChainState::default(),
            randomness: String::new(),
            current_difficulty: checkpoint.difficulty,
            base_height: anchor.index,
            checkpoint: Some(checkpoint),
        };
        chain.rebuild_state()?;
        Ok(chain)
    }

    fn genesis_state(&self) -> ChainState {
        ChainState::genesis(
            &self.genesis_treasury,
            self.genesis_validator_public_key.as_deref(),
            self.genesis_validator_vrf_public_key.as_deref(),
            self.genesis_supply,
            &self.genesis_validator_allowlist,
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
        let index = self.next_index();
        let previous_hash = self.latest_hash();
        Block::new(
            index,
            transactions,
            previous_hash,
            self.current_difficulty,
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
        // Deterministic retarget at interval boundaries.
        self.current_difficulty = Self::retarget(
            &self.blocks,
            self.tip_index(),
            self.base_height,
            self.current_difficulty,
        );
    }

    pub fn apply_finality(&mut self, finality_depth: u64) {
        if self.blocks.is_empty() {
            self.finalized_height = self.base_height;
            return;
        }
        let tip = self.tip_index();
        self.finalized_height = if finality_depth == 0 {
            tip
        } else {
            tip.saturating_sub(finality_depth).max(self.base_height)
        };
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
    /// Number of leader rounds open for a block produced at `block_ts`,
    /// given its parent's timestamp. Round r opens r × SLOT_TIMEOUT_SECONDS
    /// after the parent; the count is capped at MAX_SLOT_ROUNDS.
    fn open_rounds(parent_ts: DateTime<Utc>, block_ts: DateTime<Utc>) -> u64 {
        let elapsed = (block_ts - parent_ts).num_seconds();
        if elapsed <= 0 {
            return 0;
        }
        ((elapsed / SLOT_TIMEOUT_SECONDS) as u64).min(MAX_SLOT_ROUNDS)
    }

    fn validate_block_at(
        block: &Block,
        expected_index: u64,
        expected_previous_hash: &str,
        parent_state: &ChainState,
        parent_randomness: &str,
        parent_timestamp: DateTime<Utc>,
        expected_difficulty: usize,
    ) -> Result<ChainState, BlockError> {
        if block.index != expected_index {
            return Err(BlockError::structural(
                "Block index does not match chain tip",
            ));
        }

        // Timestamps are consensus-constrained in BOTH directions: the
        // future is bounded by MAX_TIMESTAMP_SKEW (candidate check), and a
        // block may never predate its parent — otherwise a producer could
        // backdate timestamps to manipulate difficulty retargeting.
        if block.timestamp < parent_timestamp {
            return Err(BlockError::slashable(
                "Block timestamp precedes its parent",
            ));
        }

        // Difficulty is consensus-derived (retargeting), not producer-chosen.
        if block.difficulty != expected_difficulty {
            return Err(BlockError::slashable(
                "Block difficulty does not match retarget schedule",
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

        // PoS with liveness rotation: the validator set is the ON-CHAIN set
        // at the parent state, and every slot seed comes from the VRF
        // randomness beacon — the producer of the parent block could not
        // grind it. Round 0's leader is the primary; each elapsed
        // SLOT_TIMEOUT opens the next round's leader as a fallback so an
        // offline validator can never stall the chain. The block must be
        // produced by the SMALLEST open round (per the block's own,
        // parent-bounded timestamp) that selects its validator, and its VRF
        // must verify against exactly that round's slot input.
        let validator_set = parent_state.validator_set();
        if pos::select_staker_with_seed(
            &vrf::slot_input(parent_randomness, block.index),
            &validator_set,
        )
        .is_none()
        {
            return Err(BlockError::structural(
                "No validators registered at parent state",
            ));
        }
        let claimed_validator = block.validator.as_deref().unwrap();
        let rounds = Self::open_rounds(parent_timestamp, block.timestamp);
        let mut slot_input = None;
        for round in 0..=rounds {
            let candidate = vrf::slot_input_at_round(parent_randomness, block.index, round);
            if pos::select_staker_with_seed(&candidate, &validator_set).as_deref()
                == Some(claimed_validator)
            {
                slot_input = Some(candidate);
                break;
            }
        }
        let Some(slot_input) = slot_input else {
            return Err(BlockError::slashable(
                "Block validator does not match PoS selection for any open round",
            ));
        };

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
                .apply_transaction(&tx, block.index)
                .map_err(BlockError::slashable)?;
            if tx.transaction_type == TransactionType::Reward {
                reward_count += 1;
                // Emission is consensus-enforced: the reward must equal the
                // deterministic halving schedule for this block's height.
                let expected = crate::blockchain::transaction::block_reward(block.index);
                if tx.amount != expected {
                    return Err(BlockError::slashable(
                        "Block reward does not match the emission schedule",
                    ));
                }
            }
        }
        if reward_count != 1 {
            return Err(BlockError::slashable(
                "Block must contain exactly one reward transaction",
            ));
        }

        // Block-boundary housekeeping: unbonding releases + fee payout.
        post_state.end_block(block.index, validator);

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
        let now = Utc::now();
        if block.timestamp > now + Duration::seconds(MAX_TIMESTAMP_SKEW_SECONDS) {
            return Err("Block timestamp is too far in the future".to_string());
        }
        let parent_timestamp = self
            .blocks
            .last()
            .map(|tip| tip.timestamp)
            .ok_or_else(|| "Chain is empty".to_string())?;

        Self::validate_block_at(
            block,
            self.next_index(),
            &self.latest_hash(),
            &self.state,
            &self.randomness,
            parent_timestamp,
            self.current_difficulty,
        )
        .map_err(|err| err.reason)
    }

    /// The VRF input for the next slot at the current tip. Exposed so the
    /// selected validator can compute its VRF proof offline.
    pub fn next_slot_input(&self) -> String {
        vrf::slot_input(&self.randomness, self.next_index())
    }

    /// Leader-eligibility for `validator` at the next height as of now: the
    /// smallest open round whose PoS selection picks it. Returns that
    /// round's (round, slot_input) when eligible, None otherwise. Mirrors
    /// the consensus rule in `validate_block_at`.
    pub fn eligible_slot_for(&self, validator: &str) -> Option<(u64, String)> {
        let parent_ts = self.blocks.last()?.timestamp;
        let rounds = Self::open_rounds(parent_ts, Utc::now());
        let validator_set = self.state.validator_set();
        for round in 0..=rounds {
            let input = vrf::slot_input_at_round(&self.randomness, self.next_index(), round);
            if pos::select_staker_with_seed(&input, &validator_set).as_deref() == Some(validator) {
                return Some((round, input));
            }
        }
        None
    }

    /// The leaders currently allowed to produce the next block, one per open
    /// round in priority order (round 0 = primary, later rounds = timeout
    /// fallbacks). Each entry is (round, validator, slot_input); a validator
    /// appears only at its smallest eligible round.
    pub fn open_leaders(&self) -> Vec<(u64, String, String)> {
        let Some(parent_ts) = self.blocks.last().map(|tip| tip.timestamp) else {
            return Vec::new();
        };
        let rounds = Self::open_rounds(parent_ts, Utc::now());
        let validator_set = self.state.validator_set();
        let mut leaders: Vec<(u64, String, String)> = Vec::new();
        for round in 0..=rounds {
            let input = vrf::slot_input_at_round(&self.randomness, self.next_index(), round);
            if let Some(validator) = pos::select_staker_with_seed(&input, &validator_set) {
                if !leaders.iter().any(|(_, v, _)| *v == validator) {
                    leaders.push((round, validator, input));
                }
            }
        }
        leaders
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
    /// machine, the randomness beacon, and the difficulty schedule. Returns
    /// the final (state, randomness, next difficulty), or the first failure.
    pub fn validate_full(
        &self,
    ) -> Result<(ChainState, String, usize), (usize, BlockError)> {
        let anchor = self
            .blocks
            .first()
            .ok_or((0, BlockError::structural("Chain is empty")))?;
        if !anchor.has_valid_pow() || !anchor.has_valid_merkle_root() {
            return Err((0, BlockError::structural("Root block failed validation")));
        }
        if anchor.index != self.base_height {
            return Err((
                0,
                BlockError::structural("Root block index does not match base height"),
            ));
        }

        // Seed the replay: from a trusted checkpoint, or from genesis params.
        let (mut state, mut randomness, mut difficulty) = match &self.checkpoint {
            Some(cp) => {
                if anchor.state_root != cp.state.state_root() {
                    return Err((
                        0,
                        BlockError::structural("Anchor state root does not match checkpoint"),
                    ));
                }
                (cp.state.clone(), cp.randomness.clone(), cp.difficulty)
            }
            None => {
                let g = self.genesis_state();
                if g.state_root() != anchor.state_root {
                    return Err((
                        0,
                        BlockError::structural(
                            "Genesis state root does not match network parameters",
                        ),
                    ));
                }
                (g, anchor.hash.clone(), self.difficulty)
            }
        };

        for i in 1..self.blocks.len() {
            let current = &self.blocks[i];
            let previous = &self.blocks[i - 1];
            let expected_index = self.base_height + i as u64;
            state = Self::validate_block_at(
                current,
                expected_index,
                &previous.hash,
                &state,
                &randomness,
                previous.timestamp,
                difficulty,
            )
            .map_err(|error| (i, error))?;
            randomness = vrf::next_randomness(
                &randomness,
                current.vrf_output.as_deref().unwrap_or_default(),
            );
            difficulty = Self::retarget(
                &self.blocks[..=i],
                expected_index,
                self.base_height,
                difficulty,
            );
        }
        Ok((state, randomness, difficulty))
    }

    pub fn is_valid(&self) -> bool {
        self.validate_full().is_ok()
    }

    /// Recompute `state` from the block history (after deserialization or
    /// adoption). Fails if the stored chain does not validate.
    pub fn rebuild_state(&mut self) -> Result<(), String> {
        match self.validate_full() {
            Ok((state, randomness, difficulty)) => {
                self.state = state;
                self.randomness = randomness;
                self.current_difficulty = difficulty;
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

    /// Evaluate a block (by ABSOLUTE height) for slashable behavior.
    pub fn evaluate_slash_evidence(&self, block_index: u64) -> Result<SlashEvidence, String> {
        if block_index == 0 {
            return Err("Cannot slash genesis block".to_string());
        }
        if block_index < self.base_height || block_index >= self.next_index() {
            return Err("Block index out of range".to_string());
        }
        // Map the absolute height to the local vector; the anchor (local 0)
        // is trusted and cannot be re-evaluated on a pruned chain.
        let local = (block_index - self.base_height) as usize;
        if local == 0 {
            return Err("Cannot evaluate the checkpoint anchor".to_string());
        }

        match self.validate_full() {
            Ok(_) => Err("Block does not contain slashable behavior".to_string()),
            Err((failed_local, error)) if failed_local == local && error.slashable => {
                Ok(SlashEvidence {
                    validator: self.blocks[local]
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

        // Same root: identical anchor/genesis hash AND the same base height
        // (a pruned node only fork-choices peers sharing its checkpoint).
        if our_genesis.hash != their_genesis.hash || self.base_height != candidate.base_height {
            return Err("Candidate chain has a different root".to_string());
        }

        // Finalized blocks are irreversible: the candidate must contain the
        // identical prefix up to our finalized height (local indexing).
        let finalized_local = (self.finalized_height - self.base_height) as usize;
        for local in 0..=finalized_local {
            let ours = &self.blocks[local];
            let remote = candidate
                .blocks
                .get(local)
                .ok_or_else(|| "Candidate chain rewrites finalized history".to_string())?;
            if ours.hash != remote.hash {
                return Err("Candidate chain rewrites finalized history".to_string());
            }
        }

        // A fork cannot claim time that has not passed: its tip must sit
        // within the tolerated clock skew of NOW (ancestors are bounded by
        // the tip through timestamp monotonicity). Blocks future-dating
        // their timestamps to open fallback leader rounds fail here.
        let now = Utc::now();
        if let Some(tip) = candidate.blocks.last() {
            if tip.timestamp > now + Duration::seconds(MAX_TIMESTAMP_SKEW_SECONDS) {
                return Err("Candidate chain tip timestamp is too far in the future".to_string());
            }
        }

        // SOVEREIGN-FINALITY FORK CHOICE: validator-sealed progress decides.
        // A candidate must carry MORE validator-produced blocks than we
        // have; cumulative PoW work only breaks exact height ties. Mining
        // hardware alone — however fast — can never displace blocks sealed
        // by the validator set, because every block of a heavier-work fork
        // still has to be produced by a PoS-selected, stake-bonded leader.
        let our_tip = self.tip_index();
        let their_tip = candidate.tip_index();
        if their_tip < our_tip {
            return Ok(false);
        }
        if their_tip == our_tip && candidate.cumulative_work() <= self.cumulative_work() {
            return Ok(false);
        }

        // Never trust the candidate's own genesis parameters: replay its
        // blocks under our network configuration and checkpoint root.
        let mut replay = Blockchain {
            blocks: candidate.blocks.clone(),
            difficulty: self.difficulty,
            finalized_height: self.base_height,
            genesis_treasury: self.genesis_treasury.clone(),
            genesis_validator_public_key: self.genesis_validator_public_key.clone(),
            genesis_validator_vrf_public_key: self.genesis_validator_vrf_public_key.clone(),
            genesis_supply: self.genesis_supply,
            genesis_validator_allowlist: self.genesis_validator_allowlist.clone(),
            state: ChainState::default(),
            randomness: String::new(),
            current_difficulty: self.current_difficulty,
            base_height: self.base_height,
            checkpoint: self.checkpoint.clone(),
        };
        replay
            .rebuild_state()
            .map_err(|err| format!("Candidate chain failed replay: {}", err))?;

        self.blocks = replay.blocks;
        self.state = replay.state;
        self.randomness = replay.randomness;
        self.current_difficulty = replay.current_difficulty;
        Ok(true)
    }

    /// Export a checkpoint bundle at the current tip so another node can
    /// fast-sync from here (the tip must be a retarget boundary).
    pub fn export_checkpoint(&self) -> Result<(Block, CheckpointRoot), String> {
        if self.tip_index() == 0 || self.tip_index() % RETARGET_INTERVAL != 0 {
            return Err(format!(
                "Checkpoints can only be exported when the tip height is a positive multiple of {}",
                RETARGET_INTERVAL
            ));
        }
        let anchor = self
            .blocks
            .last()
            .cloned()
            .ok_or_else(|| "Chain is empty".to_string())?;
        Ok((
            anchor,
            CheckpointRoot {
                state: self.state.clone(),
                randomness: self.randomness.clone(),
                difficulty: self.current_difficulty,
            },
        ))
    }

    /// Build a fast-sync bundle: the checkpoint at the latest retarget
    /// boundary at or below the tip, plus every block after it, so an
    /// importing node lands current.
    pub fn export_bundle(&self) -> Result<CheckpointBundle, String> {
        let tip = self.tip_index();
        let boundary = (tip / RETARGET_INTERVAL) * RETARGET_INTERVAL;
        if boundary == 0 {
            return Err("Chain is too short to checkpoint".to_string());
        }
        let boundary_local = (boundary - self.base_height) as usize;

        // Replay a truncated clone to obtain the trusted state at the boundary.
        let mut trunc = self.clone();
        trunc.blocks.truncate(boundary_local + 1);
        trunc.finalized_height = trunc.base_height;
        trunc.rebuild_state()?;
        let (anchor, checkpoint) = trunc.export_checkpoint()?;

        let forward_blocks = self.blocks[(boundary_local + 1)..].to_vec();
        Ok(CheckpointBundle {
            difficulty: self.difficulty,
            genesis_treasury: self.genesis_treasury.clone(),
            genesis_validator_public_key: self.genesis_validator_public_key.clone(),
            genesis_validator_vrf_public_key: self.genesis_validator_vrf_public_key.clone(),
            genesis_supply: self.genesis_supply,
            genesis_validator_allowlist: self.genesis_validator_allowlist.clone(),
            anchor,
            checkpoint,
            forward_blocks,
        })
    }

    /// Reconstruct a chain from a fast-sync bundle.
    pub fn from_bundle(bundle: CheckpointBundle) -> Result<Self, String> {
        Self::from_checkpoint(
            bundle.difficulty,
            bundle.genesis_treasury,
            bundle.genesis_validator_public_key,
            bundle.genesis_validator_vrf_public_key,
            bundle.genesis_supply,
            bundle.genesis_validator_allowlist,
            bundle.anchor,
            bundle.checkpoint,
            bundle.forward_blocks,
        )
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

        let next_height = chain.blocks.len() as u64;
        let mut post = chain.state.clone();
        let mut txs = Vec::new();
        for tx in &extra {
            tx.verify_for_block(&validator)?;
            post.apply_transaction(tx, next_height)?;
            txs.push(serde_json::to_string(tx).unwrap());
        }
        let reward = Transaction::new_reward(&validator, next_height);
        post.apply_transaction(&reward, next_height)?;
        txs.push(serde_json::to_string(&reward).unwrap());
        post.end_block(next_height, &validator);

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

        // Fund the new validator, then stake the minimum on-chain.
        let stake_amount = crate::blockchain::state::MIN_VALIDATOR_STAKE;
        let funded = stake_amount * 2;
        let fund = signed_transfer((&t_addr, &t_pub, &t_key), &v_addr, funded, 1);
        mine_block_with(&mut chain, vec![fund]).unwrap();

        let mut stake = Transaction::new(
            Some(v_addr.clone()),
            STAKING_POOL_ACCOUNT.to_string(),
            stake_amount,
            TransactionType::Stake,
        );
        stake.nonce = 1;
        stake.public_key = Some(v_pub.clone());
        let v_vrf = vrf::derive_vrf_public_key(&v_key).unwrap();
        stake.vrf_public_key = Some(v_vrf.clone());
        let message = Transaction::stake_signing_message(&v_addr, stake_amount, 1, &v_vrf);
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

    /// Build a reward-only block for `validator` at the next height with an
    /// explicit timestamp, signed and carrying a VRF proof over `slot_input`.
    fn build_block_at(
        chain: &Blockchain,
        validator: &str,
        private_key: &str,
        slot_input: &str,
        timestamp: chrono::DateTime<Utc>,
    ) -> Block {
        let height = chain.next_index();
        let mut post = chain.state.clone();
        let reward = Transaction::new_reward(validator, height);
        post.apply_transaction(&reward, height).unwrap();
        post.end_block(height, validator);
        let public_key = chain.state.stakers.get(validator).unwrap().public_key.clone();

        let mut block = Block::new_at(
            timestamp,
            height,
            vec![serde_json::to_string(&reward).unwrap()],
            chain.latest_hash(),
            chain.current_difficulty,
            Some(validator.to_string()),
            Some(public_key),
            None,
            post.state_root(),
        );
        let (vrf_output, vrf_proof) = vrf::prove(slot_input, private_key).unwrap();
        block.vrf_output = Some(vrf_output);
        block.vrf_proof = Some(vrf_proof);
        block.validator_signature =
            Some(pos::sign_block_hash(&block.hash, private_key).unwrap());
        block
    }

    #[test]
    fn backdated_block_timestamp_is_rejected() {
        let mut chain = Blockchain::default();
        mine_valid_block(&mut chain); // parent now has a fresh timestamp
        let (t_addr, _, t_key) = treasury();

        let parent_ts = chain.blocks.last().unwrap().timestamp;
        let input = chain.next_slot_input();
        let block = build_block_at(
            &chain,
            &t_addr,
            &t_key,
            &input,
            parent_ts - Duration::seconds(600),
        );

        let err = chain.validate_block_candidate(&block).unwrap_err();
        assert!(err.contains("precedes its parent"), "got: {err}");
    }

    #[test]
    fn fallback_leader_rotates_in_after_slot_timeout() {
        let mut chain = Blockchain::default();
        let (t_addr, t_pub, t_key) = treasury();
        let (v_addr, v_pub, v_key) = wallet(2);

        // Fund and stake a second validator with weight EQUAL to the
        // genesis validator, so both show up as leaders across rounds.
        let stake_amount = crate::blockchain::state::GENESIS_VALIDATOR_STAKE;
        let funded = stake_amount + stake_amount / 2;
        let fund = signed_transfer((&t_addr, &t_pub, &t_key), &v_addr, funded, 1);
        mine_block_with(&mut chain, vec![fund]).unwrap();
        let mut stake = Transaction::new(
            Some(v_addr.clone()),
            STAKING_POOL_ACCOUNT.to_string(),
            stake_amount,
            TransactionType::Stake,
        );
        stake.nonce = 1;
        stake.public_key = Some(v_pub.clone());
        let v_vrf = vrf::derive_vrf_public_key(&v_key).unwrap();
        stake.vrf_public_key = Some(v_vrf.clone());
        let message = Transaction::stake_signing_message(&v_addr, stake_amount, 1, &v_vrf);
        stake.signature = Some(pos::sign_message(&message, &v_key).unwrap());
        mine_block_with(&mut chain, vec![stake]).unwrap();
        assert_eq!(chain.state.validator_set().len(), 2);

        // Advance until round 0 and round 1 select DIFFERENT leaders.
        let mut guard = 0;
        loop {
            let set = chain.state.validator_set();
            let height = chain.next_index();
            let l0 = pos::select_staker_with_seed(
                &vrf::slot_input_at_round(&chain.randomness, height, 0),
                &set,
            )
            .unwrap();
            let l1 = pos::select_staker_with_seed(
                &vrf::slot_input_at_round(&chain.randomness, height, 1),
                &set,
            )
            .unwrap();
            if l0 != l1 {
                break;
            }
            mine_valid_block(&mut chain);
            guard += 1;
            assert!(guard < 200, "never found differing round leaders");
        }

        let height = chain.next_index();
        let set = chain.state.validator_set();
        let input1 = vrf::slot_input_at_round(&chain.randomness, height, 1);
        let leader1 = pos::select_staker_with_seed(&input1, &set).unwrap();
        let key1 = keyring()
            .into_iter()
            .find(|(addr, _)| *addr == leader1)
            .map(|(_, key)| key)
            .unwrap();
        let parent_ts = chain.blocks.last().unwrap().timestamp;

        // BEFORE the timeout only round 0 is open: the fallback leader's
        // block must be rejected.
        let early = build_block_at(
            &chain,
            &leader1,
            &key1,
            &input1,
            parent_ts + Duration::seconds(1),
        );
        let err = chain.validate_block_candidate(&early).unwrap_err();
        assert!(err.contains("any open round"), "got: {err}");

        // A fallback leader may not smuggle a round-0 VRF either: with the
        // timeout elapsed, its smallest eligible round is 1, so the VRF must
        // be bound to the round-1 slot input.
        let input0 = vrf::slot_input_at_round(&chain.randomness, height, 0);
        let wrong_vrf = build_block_at(
            &chain,
            &leader1,
            &key1,
            &input0,
            parent_ts + Duration::seconds(SLOT_TIMEOUT_SECONDS + 1),
        );
        assert!(chain.validate_block_candidate(&wrong_vrf).is_err());

        // AFTER the timeout, the round-1 leader with a round-1 VRF is a
        // fully valid producer — liveness is preserved.
        let late = build_block_at(
            &chain,
            &leader1,
            &key1,
            &input1,
            parent_ts + Duration::seconds(SLOT_TIMEOUT_SECONDS + 1),
        );
        let post_state = chain.validate_block_candidate(&late).expect("fallback accepted");
        chain.commit_block(late, post_state);
        assert!(chain.is_valid(), "full replay accepts the fallback block");

        // And the chain keeps extending normally afterwards (timestamps stay
        // monotonic relative to the fallback block).
        let next_height = chain.next_index();
        let set = chain.state.validator_set();
        let next_input = vrf::slot_input_at_round(&chain.randomness, next_height, 0);
        let next_leader = pos::select_staker_with_seed(&next_input, &set).unwrap();
        let next_key = keyring()
            .into_iter()
            .find(|(addr, _)| *addr == next_leader)
            .map(|(_, key)| key)
            .unwrap();
        let next_ts = chain.blocks.last().unwrap().timestamp + Duration::seconds(1);
        let next_block = build_block_at(&chain, &next_leader, &next_key, &next_input, next_ts);
        let post = chain.validate_block_candidate(&next_block).expect("chain extends");
        chain.commit_block(next_block, post);
        assert!(chain.is_valid());
    }

    #[test]
    fn rejects_forged_state_root() {
        let chain = Blockchain::default();
        let (t_addr, t_pub, t_key) = treasury();

        let reward = Transaction::new_reward(&t_addr, 1);
        let mut post = chain.state.clone();
        post.apply_transaction(&reward, 1).unwrap();
        post.end_block(1, &t_addr);
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
        let reward = Transaction::new_reward(&t_addr, 1);
        let mut post = chain.state.clone();
        post.apply_transaction(&reward, 1).unwrap();
        post.end_block(1, &t_addr);

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
    fn equal_progress_fork_is_not_adopted() {
        // Same tip height, same consensus-derived work: no displacement.
        // Validator-sealed progress — not mining effort — decides fork
        // choice, so hashrate alone can never swing a node between forks.
        let mut local = Blockchain::default();
        mine_valid_block(&mut local);
        let mut remote = Blockchain::default();
        mine_valid_block(&mut remote);
        assert!(!local.try_adopt_chain(&remote).unwrap());
        assert_eq!(local.blocks.len(), 2);
    }

    #[test]
    fn adoption_rejects_a_future_dated_fork_tip() {
        let mut local = Blockchain::default();
        mine_valid_block(&mut local);

        // Adversarial fork: longer than ours, but its tip claims a
        // timestamp beyond the tolerated clock skew (e.g. to open fallback
        // leader rounds that real time has not opened). Candidate
        // validation is bypassed to emulate a malicious peer.
        let mut remote = Blockchain::default();
        mine_valid_block(&mut remote);
        let (t_addr, _, t_key) = treasury();
        let input = remote.next_slot_input();
        let future_ts = Utc::now() + Duration::seconds(MAX_TIMESTAMP_SKEW_SECONDS + 300);
        let block = build_block_at(&remote, &t_addr, &t_key, &input, future_ts);
        let mut post = remote.state.clone();
        let reward: Transaction = serde_json::from_str(&block.transactions[0]).unwrap();
        post.apply_transaction(&reward, block.index).unwrap();
        post.end_block(block.index, &t_addr);
        remote.commit_block(block, post);

        let err = local.try_adopt_chain(&remote).unwrap_err();
        assert!(err.contains("future"), "{err}");
        assert_eq!(local.blocks.len(), 2, "fork was not adopted");
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
        let reward = Transaction::new_reward(&t_addr, 1);
        let mut post = chain.state.clone();
        post.apply_transaction(&reward, 1).unwrap();
        post.end_block(1, &t_addr);

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
        let reward = Transaction::new_reward(&t_addr, 1);
        let mut post = chain.state.clone();
        post.apply_transaction(&reward, 1).unwrap();
        post.end_block(1, &t_addr);

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
    fn difficulty_retargets_deterministically() {
        let mut chain = Blockchain::default();
        assert_eq!(chain.current_difficulty, 2);

        // Mine RETARGET_INTERVAL blocks back-to-back (far faster than the
        // 15s target) — difficulty must step up by exactly one.
        for _ in 0..RETARGET_INTERVAL {
            mine_valid_block(&mut chain);
        }
        assert_eq!(chain.current_difficulty, 3);
        assert!(chain.is_valid());

        // A candidate carrying the stale difficulty is rejected.
        let (t_addr, t_pub, t_key) = treasury();
        let next_height = chain.blocks.len() as u64;
        let reward = Transaction::new_reward(&t_addr, next_height);
        let mut post = chain.state.clone();
        post.apply_transaction(&reward, next_height).unwrap();
        post.end_block(next_height, &t_addr);
        let mut block = Block::new(
            next_height,
            vec![serde_json::to_string(&reward).unwrap()],
            chain.latest_hash(),
            2, // stale difficulty
            Some(t_addr.clone()),
            Some(t_pub),
            None,
            post.state_root(),
        );
        let (vrf_output, vrf_proof) =
            vrf::prove(&chain.next_slot_input(), &t_key).unwrap();
        block.vrf_output = Some(vrf_output);
        block.vrf_proof = Some(vrf_proof);
        block.validator_signature =
            Some(pos::sign_block_hash(&block.hash, &t_key).unwrap());
        let error = chain.validate_block_candidate(&block).unwrap_err();
        assert!(error.contains("retarget"), "unexpected error: {error}");

        // Replay from scratch converges to the same difficulty.
        let runtime = chain.current_difficulty;
        chain.rebuild_state().unwrap();
        assert_eq!(chain.current_difficulty, runtime);
    }

    #[test]
    fn checkpoint_fast_sync_matches_full_node_across_retarget() {
        // Build a full chain well past one retarget boundary.
        let mut full = Blockchain::default();
        let boundary = RETARGET_INTERVAL; // export anchor at this height
        let total = RETARGET_INTERVAL + RETARGET_INTERVAL + 3; // cross another boundary
        for _ in 0..total {
            mine_valid_block(&mut full);
        }
        assert!(full.is_valid());
        assert_eq!(full.tip_index(), total);

        // Export a checkpoint at the boundary height and take the forward
        // blocks that follow it.
        let anchor = full.blocks[boundary as usize].clone();
        assert_eq!(anchor.index, boundary);
        // Reconstruct the checkpoint state at the boundary from a truncated
        // clone validated up to it — exactly what a real exporter at that tip
        // would have committed.
        let mut trunc = full.clone();
        trunc.blocks.truncate(boundary as usize + 1);
        trunc.finalized_height = 0;
        trunc.rebuild_state().unwrap();
        let (bundle_anchor, checkpoint) = trunc.export_checkpoint().unwrap();
        assert_eq!(bundle_anchor.hash, anchor.hash);

        let forward: Vec<Block> = full.blocks[(boundary as usize + 1)..].to_vec();
        let synced = Blockchain::from_checkpoint(
            2,
            full.genesis_treasury.clone(),
            full.genesis_validator_public_key.clone(),
            full.genesis_validator_vrf_public_key.clone(),
            full.genesis_supply,
            Vec::new(),
            bundle_anchor,
            checkpoint,
            forward,
        )
        .unwrap();

        // Byte-identical convergence: same tip, state root, beacon, difficulty.
        assert_eq!(synced.tip_index(), full.tip_index());
        assert_eq!(synced.base_height, boundary);
        assert_eq!(synced.state.state_root(), full.state.state_root());
        assert_eq!(synced.randomness, full.randomness);
        assert_eq!(synced.current_difficulty, full.current_difficulty);
        assert!(synced.is_valid());
    }

    #[test]
    fn checkpoint_bundle_json_round_trip_matches_full_node() {
        // Exercise the exact production path: export_bundle() → JSON (as the
        // /checkpoint/bundle endpoint serves) → from_bundle() (as main.rs loads
        // when HIKMALAYER_CHECKPOINT is set).
        let mut full = Blockchain::default();
        let total = RETARGET_INTERVAL + RETARGET_INTERVAL + 4;
        for _ in 0..total {
            mine_valid_block(&mut full);
        }
        assert!(full.is_valid());

        let bundle = full.export_bundle().expect("bundle export");
        // The exporter must anchor on the latest retarget boundary at or below tip.
        let expected_anchor = (full.tip_index() / RETARGET_INTERVAL) * RETARGET_INTERVAL;
        assert_eq!(bundle.anchor.index, expected_anchor);

        // Serialize and deserialize through JSON, exactly like the wire path.
        let wire = serde_json::to_vec(&bundle).expect("serialize bundle");
        let decoded: CheckpointBundle = serde_json::from_slice(&wire).expect("deserialize bundle");

        let synced = Blockchain::from_bundle(decoded).expect("fast-sync from bundle");

        // Byte-identical convergence with the full node.
        assert_eq!(synced.tip_index(), full.tip_index());
        assert_eq!(synced.base_height, expected_anchor);
        assert_eq!(synced.state.state_root(), full.state.state_root());
        assert_eq!(synced.randomness, full.randomness);
        assert_eq!(synced.current_difficulty, full.current_difficulty);
        assert!(synced.is_valid());

        // The fast-synced node keeps producing valid blocks that a full node accepts.
        let mut synced = synced;
        mine_valid_block(&mut synced);
        assert!(synced.is_valid());
    }

    #[test]
    fn checkpoint_sync_rejects_a_forged_forward_block() {
        let mut full = Blockchain::default();
        for _ in 0..(RETARGET_INTERVAL + 2) {
            mine_valid_block(&mut full);
        }
        let boundary = RETARGET_INTERVAL;
        let mut trunc = full.clone();
        trunc.blocks.truncate(boundary as usize + 1);
        trunc.finalized_height = 0;
        trunc.rebuild_state().unwrap();
        let (bundle_anchor, checkpoint) = trunc.export_checkpoint().unwrap();

        // Corrupt a forward block's state root.
        let mut forward: Vec<Block> = full.blocks[(boundary as usize + 1)..].to_vec();
        forward[0].state_root = "forged".to_string();

        let result = Blockchain::from_checkpoint(
            2,
            full.genesis_treasury.clone(),
            full.genesis_validator_public_key.clone(),
            full.genesis_validator_vrf_public_key.clone(),
            full.genesis_supply,
            Vec::new(),
            bundle_anchor,
            checkpoint,
            forward,
        );
        assert!(result.is_err());
    }

    #[test]
    fn checkpoint_rejects_non_boundary_anchor() {
        let mut full = Blockchain::default();
        for _ in 0..(RETARGET_INTERVAL + 5) {
            mine_valid_block(&mut full);
        }
        // Height 3 is not a retarget boundary.
        let mut trunc = full.clone();
        trunc.blocks.truncate(4);
        trunc.finalized_height = 0;
        trunc.rebuild_state().unwrap();
        assert!(trunc.export_checkpoint().is_err());
    }

    #[test]
    fn equivocation_proof_slashes_validator_on_chain() {
        let mut chain = Blockchain::default();
        let (t_addr, t_pub, t_key) = treasury();

        // The treasury validator signs two DIFFERENT blocks at height 1.
        let reward = Transaction::new_reward(&t_addr, 1);
        let mut post = chain.state.clone();
        post.apply_transaction(&reward, 1).unwrap();
        post.end_block(1, &t_addr);
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
