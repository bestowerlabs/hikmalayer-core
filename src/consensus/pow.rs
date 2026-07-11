use sha2::{Digest, Sha256};

/// Minimum PoW difficulty accepted anywhere on the chain. A difficulty of 0
/// would make `starts_with("")` trivially true and disable PoW entirely.
pub const MIN_DIFFICULTY: usize = 1;

/// Maximum PoW difficulty. Bounds the synchronous mining loop so a
/// misconfigured (or malicious) difficulty cannot stall the node forever.
pub const MAX_DIFFICULTY: usize = 5;

pub fn clamp_difficulty(difficulty: usize) -> usize {
    difficulty.clamp(MIN_DIFFICULTY, MAX_DIFFICULTY)
}

pub fn is_difficulty_in_bounds(difficulty: usize) -> bool {
    (MIN_DIFFICULTY..=MAX_DIFFICULTY).contains(&difficulty)
}

/// Expected number of hash attempts for a given difficulty (hex leading
/// zeroes), used as the per-block work weight for fork choice.
pub fn work_for_difficulty(difficulty: usize) -> u128 {
    16u128.saturating_pow(difficulty as u32)
}

/// Tries different nonces to find a hash starting with `difficulty` zeroes.
pub fn mine_block(data: &str, difficulty: usize) -> (u64, String) {
    let difficulty = clamp_difficulty(difficulty);
    let mut nonce = 0;

    loop {
        let candidate = format!("{}{}", data, nonce);
        let mut hasher = Sha256::new();
        hasher.update(candidate.as_bytes());
        let hash = format!("{:x}", hasher.finalize());

        if hash.starts_with(&"0".repeat(difficulty)) {
            return (nonce, hash);
        }

        nonce += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamps_out_of_range_difficulty() {
        assert_eq!(clamp_difficulty(0), MIN_DIFFICULTY);
        assert_eq!(clamp_difficulty(100), MAX_DIFFICULTY);
        assert_eq!(clamp_difficulty(3), 3);
    }

    #[test]
    fn work_grows_with_difficulty() {
        assert!(work_for_difficulty(3) > work_for_difficulty(2));
        assert_eq!(work_for_difficulty(2), 256);
    }

    #[test]
    fn mined_block_satisfies_difficulty() {
        let (_, hash) = mine_block("test-data", 2);
        assert!(hash.starts_with("00"));
    }
}
