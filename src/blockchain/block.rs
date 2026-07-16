use chrono::prelude::*;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Fixed genesis timestamp (2026-01-01T00:00:00Z) so every node derives an
/// identical genesis block and chains from independent nodes can sync.
pub const GENESIS_TIMESTAMP: i64 = 1_767_225_600;
pub const GENESIS_DATA: &str = "Hikmalayer Genesis Block";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Block {
    pub index: u64,
    pub timestamp: DateTime<Utc>,
    pub transactions: Vec<String>,
    #[serde(default)]
    pub merkle_root: String,
    /// Commitment to the full chain state AFTER executing this block.
    #[serde(default)]
    pub state_root: String,
    pub previous_hash: String,
    pub difficulty: usize,
    pub nonce: u64,
    pub hash: String,
    pub validator: Option<String>,
    pub validator_public_key: Option<String>,
    pub validator_signature: Option<String>,
    /// VRF output for this slot's input, feeding the randomness beacon.
    /// Self-authenticating: for a fixed (key, input) the output is unique,
    /// so it needs no hash commitment — the proof pins it.
    #[serde(default)]
    pub vrf_output: Option<String>,
    /// Proof that `vrf_output` is the unique VRF evaluation by the
    /// validator's registered VRF key over the slot input.
    #[serde(default)]
    pub vrf_proof: Option<String>,
}

use crate::consensus::pow;

fn sha256_hex(data: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Binary Merkle tree over the transaction payloads. Odd nodes are paired
/// with themselves. An empty transaction list hashes the empty string so the
/// root is always defined.
pub fn compute_merkle_root(transactions: &[String]) -> String {
    if transactions.is_empty() {
        return sha256_hex("");
    }

    let mut layer: Vec<String> = transactions.iter().map(|tx| sha256_hex(tx)).collect();

    while layer.len() > 1 {
        let mut next = Vec::with_capacity(layer.len().div_ceil(2));
        for pair in layer.chunks(2) {
            let left = &pair[0];
            let right = pair.get(1).unwrap_or(left);
            next.push(sha256_hex(&format!("{}{}", left, right)));
        }
        layer = next;
    }

    layer.remove(0)
}

impl Block {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        index: u64,
        transactions: Vec<String>,
        previous_hash: String,
        difficulty: usize,
        validator: Option<String>,
        validator_public_key: Option<String>,
        validator_signature: Option<String>,
        state_root: String,
    ) -> Self {
        Self::new_at(
            Utc::now(),
            index,
            transactions,
            previous_hash,
            difficulty,
            validator,
            validator_public_key,
            validator_signature,
            state_root,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new_at(
        timestamp: DateTime<Utc>,
        index: u64,
        transactions: Vec<String>,
        previous_hash: String,
        difficulty: usize,
        validator: Option<String>,
        validator_public_key: Option<String>,
        validator_signature: Option<String>,
        state_root: String,
    ) -> Self {
        let difficulty = pow::clamp_difficulty(difficulty);
        let merkle_root = compute_merkle_root(&transactions);
        let data = Block::hash_payload(
            &index,
            &merkle_root,
            &state_root,
            &timestamp,
            &validator,
            &validator_public_key,
            &previous_hash,
        );

        let (nonce, hash) = pow::mine_block(&data, difficulty);

        Block {
            index,
            timestamp,
            transactions,
            merkle_root,
            state_root,
            previous_hash,
            difficulty,
            nonce,
            hash,
            validator,
            validator_public_key,
            validator_signature,
            vrf_output: None,
            vrf_proof: None,
        }
    }

    /// Deterministic genesis block committing to the genesis state root:
    /// identical on every node for the same network parameters.
    pub fn genesis(difficulty: usize, state_root: String) -> Self {
        let timestamp = DateTime::<Utc>::from_timestamp(GENESIS_TIMESTAMP, 0)
            .expect("genesis timestamp is valid");
        Self::new_at(
            timestamp,
            0,
            vec![GENESIS_DATA.to_string()],
            "0".to_string(),
            difficulty,
            None,
            None,
            None,
            state_root,
        )
    }

    fn hash_payload(
        index: &u64,
        merkle_root: &str,
        state_root: &str,
        timestamp: &DateTime<Utc>,
        validator: &Option<String>,
        validator_public_key: &Option<String>,
        previous_hash: &str,
    ) -> String {
        format!(
            "{:?}{}{}{:?}{:?}{:?}{}",
            index,
            merkle_root,
            state_root,
            timestamp,
            validator,
            validator_public_key,
            previous_hash
        )
    }

    pub fn calculate_hash(&self) -> String {
        let data = Block::hash_payload(
            &self.index,
            &self.merkle_root,
            &self.state_root,
            &self.timestamp,
            &self.validator,
            &self.validator_public_key,
            &self.previous_hash,
        );
        let candidate = format!("{}{}", data, self.nonce);
        sha256_hex(&candidate)
    }

    /// True when the merkle root actually commits to the transaction list.
    pub fn has_valid_merkle_root(&self) -> bool {
        self.merkle_root == compute_merkle_root(&self.transactions)
    }

    pub fn has_valid_pow(&self) -> bool {
        pow::is_difficulty_in_bounds(self.difficulty)
            && self.hash == self.calculate_hash()
            && self.hash.starts_with(&"0".repeat(self.difficulty))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_block_creation() {
        let block = Block::new(
            1,
            vec!["Tx".to_string()],
            "abc".to_string(),
            2,
            Some("validator-1".to_string()),
            Some("validator-pubkey".to_string()),
            None,
            "state-root".to_string(),
        );
        assert_eq!(block.index, 1);
        assert!(block.has_valid_pow());
        assert!(block.has_valid_merkle_root());
    }

    #[test]
    fn genesis_is_deterministic() {
        let a = Block::genesis(2, "root".to_string());
        let b = Block::genesis(2, "root".to_string());
        assert_eq!(a.hash, b.hash);
        assert!(a.has_valid_pow());
    }

    #[test]
    fn tampering_with_state_root_breaks_pow() {
        let mut block = Block::new(
            1,
            vec!["Tx".to_string()],
            "abc".to_string(),
            2,
            None,
            None,
            None,
            "root".to_string(),
        );
        block.state_root = "forged-root".to_string();
        assert!(!block.has_valid_pow());
    }

    #[test]
    fn tampering_with_transactions_breaks_integrity() {
        let mut block = Block::new(
            1,
            vec!["Tx".to_string()],
            "abc".to_string(),
            2,
            None,
            None,
            None,
            "root".to_string(),
        );
        block.transactions = vec!["Forged".to_string()];
        assert!(!block.has_valid_merkle_root());

        // Recomputing the merkle root still fails PoW because the root is
        // committed into the mined hash.
        block.merkle_root = compute_merkle_root(&block.transactions);
        assert!(!block.has_valid_pow());
    }

    #[test]
    fn zero_difficulty_blocks_are_rejected() {
        let mut block = Block::new(
            1,
            vec!["Tx".to_string()],
            "abc".to_string(),
            2,
            None,
            None,
            None,
            "root".to_string(),
        );
        block.difficulty = 0;
        assert!(!block.has_valid_pow());
    }
}
