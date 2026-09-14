use std::time::Duration;

use reqwest::Client;

use crate::{
    blockchain::{block::Block, chain::Blockchain, chain::ChainHead},
    p2p::protocol::{P2PEnvelope, P2PPayload},
};

#[derive(Clone)]
pub struct P2PService {
    pub node_id: String,
    pub p2p_token: Option<String>,
    /// This node's identity key. When present, every outgoing envelope is
    /// signed and `node_id` is the address derived from it.
    node_private_key: Option<String>,
    client: Client,
    max_retries: usize,
}

impl P2PService {
    pub fn new(node_id: String, p2p_token: Option<String>) -> Result<Self, String> {
        Self::with_identity(node_id, p2p_token, None)
    }

    pub fn with_identity(
        node_id: String,
        p2p_token: Option<String>,
        node_private_key: Option<String>,
    ) -> Result<Self, String> {
        let client = Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .map_err(|e| format!("Failed to build P2P client: {}", e))?;

        // Derive the node_id from the identity key when one is configured.
        let node_id = match &node_private_key {
            Some(key) => {
                let public = crate::consensus::pos::derive_public_key(key)?;
                crate::consensus::pos::derive_address(&public)?
            }
            None => node_id,
        };

        Ok(Self {
            node_id,
            p2p_token,
            node_private_key,
            client,
            max_retries: 2,
        })
    }

    /// Sign an envelope with the node identity key when one is configured.
    fn finalize(&self, envelope: P2PEnvelope) -> P2PEnvelope {
        match &self.node_private_key {
            Some(key) => envelope.signed(key).unwrap_or_else(|_| {
                P2PEnvelope::new(self.node_id.clone(), P2PPayload::Ping)
            }),
            None => envelope,
        }
    }

    pub fn block_envelope(&self, block: Block) -> P2PEnvelope {
        self.finalize(P2PEnvelope::new(self.node_id.clone(), P2PPayload::Block(Box::new(block))))
    }

    pub async fn broadcast_block(&self, peers: Vec<String>, block: Block) -> (u64, u64) {
        self.broadcast_envelope(peers, self.block_envelope(block))
            .await
    }

    /// Gossip a pending transaction so any selected validator can mine it.
    pub async fn broadcast_transaction(
        &self,
        peers: Vec<String>,
        transaction: crate::blockchain::transaction::Transaction,
    ) -> (u64, u64) {
        let envelope = self.finalize(P2PEnvelope::new(
            self.node_id.clone(),
            P2PPayload::Transaction(Box::new(transaction)),
        ));
        self.broadcast_envelope(peers, envelope).await
    }

    async fn broadcast_envelope(&self, peers: Vec<String>, envelope: P2PEnvelope) -> (u64, u64) {
        let mut sent = 0u64;
        let mut failed = 0u64;

        for peer in peers {
            let ok = self.send_with_retry(&peer, &envelope).await;
            if ok {
                sent += 1;
            } else {
                failed += 1;
            }
        }

        (sent, failed)
    }

    async fn send_with_retry(&self, peer: &str, envelope: &P2PEnvelope) -> bool {
        for attempt in 0..=self.max_retries {
            if self.send_once(peer, envelope).await {
                return true;
            }

            if attempt < self.max_retries {
                tokio::time::sleep(Duration::from_millis(200 * (attempt as u64 + 1))).await;
            }
        }

        false
    }

    /// Tell a peer we exist, so its gossip reaches us.
    ///
    /// Without this a fresh node is invisible: it knows its bootnode from
    /// configuration, but the bootnode has never heard of it, so no block is
    /// ever pushed its way. Announcing is what makes the link bidirectional.
    pub async fn announce_self(&self, peer: &str, our_address: &str) -> bool {
        let envelope = self.finalize(P2PEnvelope::new(
            self.node_id.clone(),
            P2PPayload::PeerAnnounce {
                address: our_address.to_string(),
            },
        ));
        self.send_once(peer, &envelope).await
    }

    /// Ask a peer which peers it knows, so the network can grow past the
    /// bootnodes a node happened to be configured with.
    pub async fn fetch_peers(&self, peer: &str) -> Option<Vec<String>> {
        let url = format!("{}/p2p/peers", peer.trim_end_matches('/'));
        let response = self.authorized(self.client.get(url)).send().await.ok()?;
        if !response.status().is_success() {
            return None;
        }
        response.json::<Vec<String>>().await.ok()
    }

    /// Where a peer's chain stands, without downloading it.
    pub async fn fetch_head(&self, peer: &str) -> Option<ChainHead> {
        let url = format!("{}/p2p/head", peer.trim_end_matches('/'));
        let response = self.authorized(self.client.get(url)).send().await.ok()?;
        if !response.status().is_success() {
            return None;
        }
        response.json::<ChainHead>().await.ok()
    }

    /// Fetch a bounded run of blocks starting at an absolute height.
    ///
    /// The peer caps the count regardless of what we ask for, so a long
    /// history arrives as a sequence of bounded responses rather than one
    /// enormous one.
    pub async fn fetch_blocks_from(
        &self,
        peer: &str,
        from_height: u64,
        limit: usize,
    ) -> Option<Vec<Block>> {
        let url = format!(
            "{}/p2p/blocks/{}?limit={}",
            peer.trim_end_matches('/'),
            from_height,
            limit
        );
        // Block ranges are much larger than a head poll, so they get their
        // own, longer timeout rather than the default used for small calls.
        let response = self
            .authorized(self.client.get(url))
            .timeout(Duration::from_secs(30))
            .send()
            .await
            .ok()?;
        if !response.status().is_success() {
            return None;
        }
        response.json::<Vec<Block>>().await.ok()
    }

    /// Attach the P2P bearer token, when this node has one.
    fn authorized(&self, request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match &self.p2p_token {
            Some(token) => request.header("x-p2p-token", token),
            None => request,
        }
    }

    /// Fetch a peer's full chain for fork-choice evaluation.
    pub async fn fetch_chain(&self, peer: &str) -> Option<Blockchain> {
        let url = format!("{}/p2p/chain", peer.trim_end_matches('/'));
        let mut request = self.client.get(url);

        if let Some(token) = &self.p2p_token {
            request = request.header("x-p2p-token", token);
        }

        let response = request.send().await.ok()?;
        if !response.status().is_success() {
            return None;
        }
        response.json::<Blockchain>().await.ok()
    }

    async fn send_once(&self, peer: &str, envelope: &P2PEnvelope) -> bool {
        let url = format!("{}/p2p/protocol", peer.trim_end_matches('/'));
        let mut request = self.client.post(url).json(envelope);

        if let Some(token) = &self.p2p_token {
            request = request.header("x-p2p-token", token);
        }

        match request.send().await {
            Ok(response) => response.status().is_success(),
            Err(_) => false,
        }
    }
}
