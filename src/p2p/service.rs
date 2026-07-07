use std::time::Duration;

use reqwest::Client;

use crate::{
    blockchain::{block::Block, chain::Blockchain},
    p2p::protocol::{P2PEnvelope, P2PPayload},
};

#[derive(Clone)]
pub struct P2PService {
    pub node_id: String,
    pub p2p_token: Option<String>,
    client: Client,
    max_retries: usize,
}

impl P2PService {
    pub fn new(node_id: String, p2p_token: Option<String>) -> Result<Self, String> {
        let client = Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .map_err(|e| format!("Failed to build P2P client: {}", e))?;

        Ok(Self {
            node_id,
            p2p_token,
            client,
            max_retries: 2,
        })
    }

    pub fn block_envelope(&self, block: Block) -> P2PEnvelope {
        P2PEnvelope::new(self.node_id.clone(), P2PPayload::Block(block))
    }

    pub async fn broadcast_block(&self, peers: Vec<String>, block: Block) -> (u64, u64) {
        self.broadcast_envelope(peers, self.block_envelope(block))
            .await
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
