use super::block::Block;
use super::transaction::{Transaction, TransactionType};
use crate::consensus::{
    pos::{self, Staker},
    pow,
};
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};

/// Maximum tolerated clock skew (seconds) for incoming block timestamps.
pub const MAX_TIMESTAMP_SKEW_SECONDS: i64 = 120;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Blockchain {
    pub blocks: Vec<Block>,
    pub difficulty: usize,
    #[serde(default)]
    pub finalized_height: u64,
}

/// A validation failure. `slashable` distinguishes provable validator
/// misbehavior (wrong slot, bad signature, bad PoW, tampered payload) from
/// structural problems (missing fields, stale tip).
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
    pub fn new(difficulty: usize) -> Self {
        let difficulty = pow::clamp_difficulty(difficulty);
        let genesis_block = Block::genesis(difficulty);
        Blockchain {
            blocks: vec![genesis_block],
            difficulty,
            finalized_height: 0,
        }
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
        staker_set_hash: Option<String>,
        staker_snapshot: Option<Vec<Staker>>,
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
            staker_set_hash,
            staker_snapshot,
        )
    }

    pub fn add_mined_block(&mut self, block: Block) {
        self.blocks.push(block);
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
    /// expected position: linkage, PoS selection, validator signature,
    /// merkle integrity, and PoW.
    fn validate_block_at(
        block: &Block,
        expected_index: u64,
        expected_previous_hash: &str,
    ) -> Result<(), BlockError> {
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
            || block.staker_snapshot.is_none()
            || block.staker_set_hash.is_none()
        {
            return Err(BlockError::structural(
                "Block missing required validator or staker data",
            ));
        }

        let staker_snapshot = block.staker_snapshot.as_ref().unwrap();
        let staker_hash = pos::staker_set_hash(staker_snapshot);
        if Some(staker_hash) != block.staker_set_hash {
            return Err(BlockError::slashable("Block staker set hash mismatch"));
        }

        let seed = pos::selection_seed(&block.previous_hash, block.index);
        let expected_validator = pos::select_staker_with_seed(&seed, staker_snapshot);
        if expected_validator != block.validator {
            return Err(BlockError::slashable(
                "Block validator does not match PoS selection",
            ));
        }

        // The block validator's registered key must be the one that signed.
        let validator = block.validator.as_ref().unwrap();
        let public_key = block.validator_public_key.as_ref().unwrap();
        let registered_key = staker_snapshot
            .iter()
            .find(|staker| &staker.address == validator)
            .and_then(|staker| staker.public_key.as_ref());
        if registered_key != Some(public_key) {
            return Err(BlockError::slashable(
                "Block signer key does not match validator's registered key",
            ));
        }

        let signature = block.validator_signature.as_ref().unwrap();
        if !pos::verify_block_signature(&block.hash, public_key, signature) {
            return Err(BlockError::slashable("Block signature verification failed"));
        }

        if !block.has_valid_merkle_root() {
            return Err(BlockError::slashable("Block merkle root mismatch"));
        }

        if !block.has_valid_pow() {
            return Err(BlockError::slashable("Block PoW validation failed"));
        }

        // Transaction-level consensus rules: every payload must be a
        // well-formed transaction, transfers must carry valid sender
        // signatures, and the block may pay at most one correct reward.
        let mut reward_count = 0usize;
        for tx_str in &block.transactions {
            let tx: Transaction = serde_json::from_str(tx_str)
                .map_err(|_| BlockError::slashable("Block contains malformed transaction"))?;
            tx.verify_for_block(validator)
                .map_err(BlockError::slashable)?;
            if tx.transaction_type == TransactionType::Reward {
                reward_count += 1;
                if reward_count > 1 {
                    return Err(BlockError::slashable(
                        "Block contains multiple reward transactions",
                    ));
                }
            }
        }

        Ok(())
    }

    /// Validate a candidate block for appending to the current tip. Adds
    /// tip-difficulty and timestamp-freshness rules on top of the consensus
    /// checks.
    pub fn validate_block_candidate(&self, block: &Block) -> Result<(), String> {
        if block.difficulty != self.difficulty {
            return Err("Block difficulty does not match chain difficulty".to_string());
        }

        let now = Utc::now();
        if block.timestamp > now + Duration::seconds(MAX_TIMESTAMP_SKEW_SECONDS) {
            return Err("Block timestamp is too far in the future".to_string());
        }

        Self::validate_block_at(block, self.blocks.len() as u64, &self.latest_hash())
            .map_err(|err| err.reason)
    }

    pub fn evaluate_slash_evidence(&self, block_index: u64) -> Result<SlashEvidence, String> {
        let index = block_index as usize;
        if index == 0 {
            return Err("Cannot slash genesis block".to_string());
        }
        if index >= self.blocks.len() {
            return Err("Block index out of range".to_string());
        }

        let block = &self.blocks[index];
        let previous = &self.blocks[index - 1];

        match Self::validate_block_at(block, block.index, &previous.hash) {
            Ok(()) => Err("Block does not contain slashable behavior".to_string()),
            Err(error) => Ok(SlashEvidence {
                validator: block
                    .validator
                    .clone()
                    .unwrap_or_else(|| "unknown".to_string()),
                reason: error.reason,
                timestamp: Utc::now().to_rfc3339(),
            }),
        }
    }

    pub fn is_valid(&self) -> bool {
        self.validate_full().is_ok()
    }

    /// Validate the entire chain from genesis. Returns the first failure.
    pub fn validate_full(&self) -> Result<(), (usize, BlockError)> {
        match self.blocks.first() {
            Some(genesis) => {
                if !genesis.has_valid_pow() || !genesis.has_valid_merkle_root() {
                    return Err((0, BlockError::structural("Genesis block failed validation")));
                }
            }
            None => return Err((0, BlockError::structural("Chain is empty"))),
        }

        for i in 1..self.blocks.len() {
            let current = &self.blocks[i];
            let previous = &self.blocks[i - 1];
            Self::validate_block_at(current, i as u64, &previous.hash)
                .map_err(|error| (i, error))?;
        }
        Ok(())
    }

    pub fn validate_and_slash(
        &self,
        stakers: &mut Vec<Staker>,
    ) -> (bool, Vec<(String, u64)>, Option<String>) {
        let mut slashed = Vec::new();

        match self.validate_full() {
            Ok(()) => (true, slashed, None),
            Err((index, error)) => {
                if error.slashable {
                    if let Some(validator) = self
                        .blocks
                        .get(index)
                        .and_then(|block| block.validator.clone())
                    {
                        let amount = pos::slash_staker(stakers, &validator);
                        if amount > 0 {
                            slashed.push((validator, amount));
                        }
                    }
                }
                (
                    false,
                    slashed,
                    Some(format!("Block {}: {}", index, error.reason)),
                )
            }
        }
    }

    /// Fork choice: adopt `candidate` if it shares our genesis, is fully
    /// valid, preserves every finalized block, and carries strictly more
    /// cumulative work. Returns Ok(true) when the chain was replaced.
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

        candidate
            .validate_full()
            .map_err(|(index, error)| format!("Candidate block {}: {}", index, error.reason))?;

        self.blocks = candidate.blocks.clone();
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

    fn test_keys() -> (String, String) {
        let private_key = hex::encode([1u8; 32]);
        let public_key = pos::derive_public_key(&private_key).unwrap();
        (public_key, private_key)
    }

    fn test_stakers(public_key: &str) -> Vec<Staker> {
        vec![Staker {
            address: "validator-1".to_string(),
            stake: 10,
            public_key: Some(public_key.to_string()),
        }]
    }

    fn reward_txs(validator: &str) -> Vec<String> {
        vec![serde_json::to_string(&Transaction::new_reward(validator)).unwrap()]
    }

    /// Produce and append a fully valid signed block carrying its reward tx.
    fn mine_valid_block(chain: &mut Blockchain) {
        let (public_key, private_key) = test_keys();
        let stakers = test_stakers(&public_key);
        let staker_hash = pos::staker_set_hash(&stakers);
        let mut block = chain.create_block(
            reward_txs("validator-1"),
            Some("validator-1".to_string()),
            Some(public_key),
            Some(staker_hash),
            Some(stakers),
        );
        let signature = pos::sign_block_hash(&block.hash, &private_key).unwrap();
        block.validator_signature = Some(signature);
        chain
            .validate_block_candidate(&block)
            .expect("test block should be valid");
        chain.add_mined_block(block);
    }

    #[test]
    fn test_blockchain_addition() {
        let mut chain = Blockchain::default();
        assert_eq!(chain.blocks.len(), 1);
        mine_valid_block(&mut chain);
        assert_eq!(chain.blocks.len(), 2);
    }

    #[test]
    fn test_chain_validation() {
        let mut chain = Blockchain::default();
        mine_valid_block(&mut chain);
        mine_valid_block(&mut chain);
        assert!(chain.is_valid());
    }

    #[test]
    fn rejects_unsigned_candidate() {
        let chain = Blockchain::default();
        let (public_key, _) = test_keys();
        let stakers = test_stakers(&public_key);
        let staker_hash = pos::staker_set_hash(&stakers);
        let block = chain.create_block(
            reward_txs("validator-1"),
            Some("validator-1".to_string()),
            Some(public_key),
            Some(staker_hash),
            Some(stakers),
        );
        assert!(chain.validate_block_candidate(&block).is_err());
    }

    #[test]
    fn rejects_wrong_validator_slot() {
        let chain = Blockchain::default();
        let (public_key, private_key) = test_keys();
        // Two stakers; claim the block for whichever one PoS did NOT select.
        let stakers = vec![
            Staker {
                address: "validator-1".to_string(),
                stake: 10,
                public_key: Some(public_key.clone()),
            },
            Staker {
                address: "validator-2".to_string(),
                stake: 10,
                public_key: Some(public_key.clone()),
            },
        ];
        let seed = pos::selection_seed(&chain.latest_hash(), 1);
        let selected = pos::select_staker_with_seed(&seed, &stakers).unwrap();
        let imposter = if selected == "validator-1" {
            "validator-2"
        } else {
            "validator-1"
        };

        let staker_hash = pos::staker_set_hash(&stakers);
        let mut block = chain.create_block(
            vec!["Tx1".to_string()],
            Some(imposter.to_string()),
            Some(public_key),
            Some(staker_hash),
            Some(stakers),
        );
        block.validator_signature =
            Some(pos::sign_block_hash(&block.hash, &private_key).unwrap());

        let error = chain.validate_block_candidate(&block).unwrap_err();
        assert!(error.contains("PoS selection"), "unexpected error: {error}");
    }

    #[test]
    fn rejects_signer_key_not_registered_for_validator() {
        let chain = Blockchain::default();
        let (registered_key, _) = test_keys();
        let attacker_private = hex::encode([9u8; 32]);
        let attacker_public = pos::derive_public_key(&attacker_private).unwrap();

        let stakers = test_stakers(&registered_key);
        let staker_hash = pos::staker_set_hash(&stakers);
        let mut block = chain.create_block(
            reward_txs("validator-1"),
            Some("validator-1".to_string()),
            Some(attacker_public),
            Some(staker_hash),
            Some(stakers),
        );
        block.validator_signature =
            Some(pos::sign_block_hash(&block.hash, &attacker_private).unwrap());

        let error = chain.validate_block_candidate(&block).unwrap_err();
        assert!(error.contains("registered key"), "unexpected error: {error}");
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
    fn fork_choice_adopts_heavier_chain() {
        let mut local = Blockchain::default();
        mine_valid_block(&mut local);

        let mut remote = Blockchain::default();
        mine_valid_block(&mut remote);
        mine_valid_block(&mut remote);
        mine_valid_block(&mut remote);

        let adopted = local.try_adopt_chain(&remote).unwrap();
        assert!(adopted);
        assert_eq!(local.blocks.len(), 4);
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
        // Finalize everything mined so far.
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
        let (public_key, private_key) = test_keys();
        let stakers = test_stakers(&public_key);
        let staker_hash = pos::staker_set_hash(&stakers);
        let mut block = chain.create_block(
            reward_txs("validator-1"),
            Some("validator-1".to_string()),
            Some(public_key),
            Some(staker_hash),
            Some(stakers),
        );
        block.timestamp = Utc::now() + Duration::seconds(MAX_TIMESTAMP_SKEW_SECONDS + 60);
        block.validator_signature =
            Some(pos::sign_block_hash(&block.hash, &private_key).unwrap());

        let error = chain.validate_block_candidate(&block).unwrap_err();
        assert!(error.contains("future"), "unexpected error: {error}");
    }

    #[test]
    fn validate_and_slash_penalizes_bad_signature() {
        let mut chain = Blockchain::default();
        mine_valid_block(&mut chain);
        // Corrupt the signature after acceptance.
        chain.blocks[1].validator_signature = Some(hex::encode([0u8; 64]));

        let (public_key, _) = test_keys();
        let mut stakers = test_stakers(&public_key);
        let (valid, slashed, details) = chain.validate_and_slash(&mut stakers);
        assert!(!valid);
        assert_eq!(slashed.len(), 1);
        assert_eq!(slashed[0].0, "validator-1");
        assert!(details.is_some());
        assert_eq!(stakers[0].stake, 9);
    }
}
