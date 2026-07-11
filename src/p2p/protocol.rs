use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashSet, VecDeque};

use crate::blockchain::{block::Block, transaction::Transaction};
use crate::consensus::pos;

/// Bounded set of recently seen message IDs used to reject replayed P2P
/// envelopes. Oldest entries are evicted first.
#[derive(Debug, Default)]
pub struct SeenMessageCache {
    seen: HashSet<String>,
    order: VecDeque<String>,
    capacity: usize,
}

impl SeenMessageCache {
    pub fn new(capacity: usize) -> Self {
        Self {
            seen: HashSet::new(),
            order: VecDeque::new(),
            capacity: capacity.max(1),
        }
    }

    /// Record a message ID. Returns false when the ID was already seen
    /// (i.e. the message is a replay).
    pub fn insert(&mut self, message_id: &str) -> bool {
        if self.seen.contains(message_id) {
            return false;
        }
        if self.order.len() >= self.capacity {
            if let Some(evicted) = self.order.pop_front() {
                self.seen.remove(&evicted);
            }
        }
        self.seen.insert(message_id.to_string());
        self.order.push_back(message_id.to_string());
        true
    }
}

pub const P2P_PROTOCOL_VERSION: &str = "hikmalayer-p2p/1";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct P2PEnvelope {
    pub protocol_version: String,
    pub node_id: String,
    pub message_id: String,
    pub timestamp: DateTime<Utc>,
    pub payload: P2PPayload,
    /// Sender's node public key (hex secp256k1). `node_id` must be the
    /// address derived from it — this binds the envelope to a node identity.
    #[serde(default)]
    pub node_public_key: Option<String>,
    /// Signature over the canonical envelope digest by the node key.
    #[serde(default)]
    pub signature: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum P2PPayload {
    Ping,
    PeerAnnounce { address: String },
    Block(Block),
    BlockBatch(Vec<Block>),
    Transaction(Transaction),
}

impl P2PEnvelope {
    pub fn new(node_id: String, payload: P2PPayload) -> Self {
        Self {
            protocol_version: P2P_PROTOCOL_VERSION.to_string(),
            node_id,
            message_id: uuid::Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            payload,
            node_public_key: None,
            signature: None,
        }
    }

    /// Canonical digest bound by the node signature: every header field plus
    /// the serialized payload. A change to any of them invalidates the
    /// signature.
    pub fn signing_digest(&self) -> String {
        let payload = serde_json::to_string(&self.payload).unwrap_or_default();
        let material = format!(
            "{}|{}|{}|{}|{}",
            self.protocol_version,
            self.node_id,
            self.message_id,
            self.timestamp.to_rfc3339(),
            payload
        );
        format!("{:x}", Sha256::digest(material.as_bytes()))
    }

    /// Sign the envelope with this node's private key, setting `node_id` to
    /// the derived address and stamping the public key + signature.
    pub fn signed(mut self, private_key_hex: &str) -> Result<Self, String> {
        let public_key = pos::derive_public_key(private_key_hex)?;
        self.node_id = pos::derive_address(&public_key)?;
        let signature = pos::sign_message(&self.signing_digest(), private_key_hex)?;
        self.node_public_key = Some(public_key);
        self.signature = Some(signature);
        Ok(self)
    }

    /// Verify the node handshake: the public key must derive to `node_id`
    /// and the signature must cover the canonical digest.
    pub fn verify_identity(&self) -> Result<(), String> {
        let public_key = self
            .node_public_key
            .as_ref()
            .ok_or_else(|| "Envelope missing node public key".to_string())?;
        let signature = self
            .signature
            .as_ref()
            .ok_or_else(|| "Envelope missing node signature".to_string())?;
        let derived = pos::derive_address(public_key)?;
        if derived != self.node_id {
            return Err("Envelope node_id does not match node public key".to_string());
        }
        if !pos::verify_message(&self.signing_digest(), public_key, signature) {
            return Err("Envelope signature verification failed".to_string());
        }
        Ok(())
    }

    /// Structural + freshness validation. When `require_identity` is set, the
    /// envelope must also carry a valid node handshake signature.
    pub fn validate(&self, max_clock_skew_seconds: i64) -> Result<(), String> {
        self.validate_with_identity(max_clock_skew_seconds, false)
    }

    pub fn validate_with_identity(
        &self,
        max_clock_skew_seconds: i64,
        require_identity: bool,
    ) -> Result<(), String> {
        if self.protocol_version != P2P_PROTOCOL_VERSION {
            return Err("Unsupported P2P protocol version".to_string());
        }

        if self.node_id.trim().is_empty() {
            return Err("Missing node_id in P2P envelope".to_string());
        }

        if self.message_id.trim().is_empty() {
            return Err("Missing message_id in P2P envelope".to_string());
        }

        let now = Utc::now();
        let skew = (now - self.timestamp).num_seconds().abs();
        if skew > max_clock_skew_seconds {
            return Err(format!(
                "P2P envelope timestamp exceeds allowed skew ({}s)",
                max_clock_skew_seconds
            ));
        }

        if require_identity || self.signature.is_some() {
            self.verify_identity()?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seen_cache_rejects_replays_and_evicts() {
        let mut cache = SeenMessageCache::new(2);
        assert!(cache.insert("a"));
        assert!(!cache.insert("a"));
        assert!(cache.insert("b"));
        assert!(cache.insert("c")); // evicts "a"
        assert!(cache.insert("a"));
        assert!(!cache.insert("c"));
    }

    #[test]
    fn validates_fresh_envelope() {
        let env = P2PEnvelope::new("node-a".to_string(), P2PPayload::Ping);
        assert!(env.validate(60).is_ok());
    }

    #[test]
    fn rejects_bad_version() {
        let mut env = P2PEnvelope::new("node-a".to_string(), P2PPayload::Ping);
        env.protocol_version = "bad/0".to_string();
        assert!(env.validate(60).is_err());
    }

    #[test]
    fn signed_envelope_roundtrips_and_binds_identity() {
        let private_key = hex::encode([4u8; 32]);
        let env = P2PEnvelope::new("placeholder".to_string(), P2PPayload::Ping)
            .signed(&private_key)
            .unwrap();
        // node_id was set to the derived address; identity verifies.
        assert!(env.verify_identity().is_ok());
        assert!(env.validate_with_identity(60, true).is_ok());

        // Tampering with the payload breaks the signature.
        let mut tampered = env.clone();
        tampered.payload = P2PPayload::PeerAnnounce {
            address: "evil".to_string(),
        };
        assert!(tampered.verify_identity().is_err());

        // Spoofing node_id (claiming another identity) is rejected.
        let mut spoofed = env.clone();
        spoofed.node_id = "hkmsomeoneelse".to_string();
        assert!(spoofed.verify_identity().is_err());
    }

    #[test]
    fn require_identity_rejects_unsigned() {
        let env = P2PEnvelope::new("node-a".to_string(), P2PPayload::Ping);
        assert!(env.validate_with_identity(60, true).is_err());
    }
}
