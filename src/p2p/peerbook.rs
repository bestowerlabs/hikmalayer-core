//! Peer reputation, scoring, and banning.
//!
//! Peers are cryptographically identified by their signed `node_id` (see
//! `protocol::P2PEnvelope`). This book tracks a reputation score per node:
//! useful contributions (valid blocks/transactions) raise it, misbehavior
//! (invalid or malformed messages) lowers it, and a node that drops below the
//! ban threshold is refused until its ban expires. An optional allow-list
//! restricts participation to explicitly permitted validator node keys.

use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::time::{SystemTime, UNIX_EPOCH};

/// Starting reputation for a newly seen peer.
pub const INITIAL_SCORE: i64 = 0;

/// Score at or below which a peer is banned.
pub const BAN_THRESHOLD: i64 = -10;

/// Reward for a useful contribution (valid block or transaction accepted).
pub const GOOD_DELTA: i64 = 1;

/// Penalty for a misbehaving message (invalid/malformed/replayed).
pub const BAD_DELTA: i64 = -4;

/// How long (seconds) a ban lasts before the peer may be retried.
pub const BAN_SECONDS: u64 = 600;

/// Score bounds so reputation cannot run away in either direction.
pub const SCORE_MIN: i64 = -50;
pub const SCORE_MAX: i64 = 50;

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[derive(Debug, Clone, Serialize)]
pub struct PeerRecord {
    pub node_id: String,
    pub score: i64,
    pub good: u64,
    pub bad: u64,
    /// Unix time the current ban expires (0 = not banned).
    pub banned_until: u64,
}

#[derive(Debug, Default)]
pub struct PeerBook {
    peers: HashMap<String, PeerRecord>,
    /// When non-empty, only these node_ids may participate.
    allow_list: HashSet<String>,
}

impl PeerBook {
    pub fn new() -> Self {
        Self::default()
    }

    /// Restrict participation to an explicit set of validator node ids.
    pub fn with_allow_list(allow_list: HashSet<String>) -> Self {
        Self {
            peers: HashMap::new(),
            allow_list,
        }
    }

    pub fn has_allow_list(&self) -> bool {
        !self.allow_list.is_empty()
    }

    fn entry(&mut self, node_id: &str) -> &mut PeerRecord {
        self.peers
            .entry(node_id.to_string())
            .or_insert_with(|| PeerRecord {
                node_id: node_id.to_string(),
                score: INITIAL_SCORE,
                good: 0,
                bad: 0,
                banned_until: 0,
            })
    }

    /// Whether a node may currently be accepted: not allow-list-excluded and
    /// not under an active ban.
    pub fn is_accepted(&self, node_id: &str) -> bool {
        if self.has_allow_list() && !self.allow_list.contains(node_id) {
            return false;
        }
        match self.peers.get(node_id) {
            Some(record) => record.banned_until <= now_secs(),
            None => true,
        }
    }

    /// Record a useful contribution.
    pub fn record_good(&mut self, node_id: &str) {
        let record = self.entry(node_id);
        record.good += 1;
        record.score = (record.score + GOOD_DELTA).min(SCORE_MAX);
    }

    /// Record misbehavior. Returns true if this pushed the peer into a ban.
    pub fn record_bad(&mut self, node_id: &str) -> bool {
        let record = self.entry(node_id);
        record.bad += 1;
        record.score = (record.score + BAD_DELTA).max(SCORE_MIN);
        if record.score <= BAN_THRESHOLD && record.banned_until <= now_secs() {
            record.banned_until = now_secs() + BAN_SECONDS;
            // Reset toward the threshold so the peer gets a bounded second
            // chance after the ban rather than instant re-ban.
            record.score = BAN_THRESHOLD / 2;
            return true;
        }
        false
    }

    pub fn is_banned(&self, node_id: &str) -> bool {
        self.peers
            .get(node_id)
            .map(|r| r.banned_until > now_secs())
            .unwrap_or(false)
    }

    pub fn snapshot(&self) -> Vec<PeerRecord> {
        let mut records: Vec<PeerRecord> = self.peers.values().cloned().collect();
        records.sort_by(|a, b| b.score.cmp(&a.score).then(a.node_id.cmp(&b.node_id)));
        records
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn good_and_bad_move_the_score() {
        let mut book = PeerBook::new();
        book.record_good("node-a");
        book.record_good("node-a");
        let rec = &book.snapshot()[0];
        assert_eq!(rec.score, 2);
        assert_eq!(rec.good, 2);
    }

    #[test]
    fn repeated_misbehavior_bans_the_peer() {
        let mut book = PeerBook::new();
        assert!(book.is_accepted("bad"));
        // 3 bad = -12 <= -10 threshold → banned on the third.
        assert!(!book.record_bad("bad"));
        assert!(!book.record_bad("bad"));
        assert!(book.record_bad("bad"));
        assert!(book.is_banned("bad"));
        assert!(!book.is_accepted("bad"));
    }

    #[test]
    fn allow_list_excludes_unlisted_nodes() {
        let mut allowed = HashSet::new();
        allowed.insert("validator-1".to_string());
        let book = PeerBook::with_allow_list(allowed);
        assert!(book.is_accepted("validator-1"));
        assert!(!book.is_accepted("stranger"));
    }
}
