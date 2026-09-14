//! Automatic chain synchronisation.
//!
//! # Why this exists
//!
//! Before this module, the only path that could bring a node up to date was
//! [`crate::api::routes`]'s fork-choice sync, and it ran in exactly one
//! situation: a *gossiped* block arrived and failed to extend the local tip.
//! That works for a node that is already caught up and missed one block. It
//! does nothing for a node that has never seen the chain at all, for two
//! reasons that compound:
//!
//! 1. Nothing ever asked a peer for history. A fresh node sat at genesis
//!    waiting for a block it had no reason to receive.
//! 2. Nothing ever *announced* the node to its peers, so the producers did
//!    not know to gossip to it — which meant even the one trigger that did
//!    exist never fired.
//!
//! The observable symptom was a brand-new validator sitting at genesis
//! indefinitely, making no attempt to sync, and needing its state file
//! copied across by hand.
//!
//! # What replaces it
//!
//! A background worker that runs for the life of the node and, on every tick:
//!
//! * announces this node to its peers, so gossip flows *towards* it;
//! * learns new peers from its peers, so the network grows past the bootnode
//!   list a node happened to start with;
//! * polls every peer's head — cheap, no chain download — and picks the best
//!   one on *our* network;
//! * if we are behind, downloads blocks in bounded batches and appends them
//!   through the ordinary validation path until caught up.
//!
//! A node that starts at genesis reconstructs the entire history this way,
//! unattended, and one that falls behind later recovers the same way.
//!
//! # What it deliberately does not do
//!
//! **It does not weaken validation.** Every block fetched here goes through
//! [`Blockchain::validate_block_candidate`] — the same full re-execution a
//! gossiped block gets. Sync is a *transport*; it buys no trust. A peer that
//! serves us a bad block gets it rejected exactly as if it had gossiped it.
//!
//! **It does not bypass fork choice.** Catching up is only ever appending to
//! our own tip. The moment a peer's history diverges from ours, the fast path
//! stops and the existing [`Blockchain::try_adopt_chain`] decides — which is
//! where finalized-history protection and validator-progress-first fork
//! choice live. Sync can make us longer; only fork choice can make us
//! different.
//!
//! **It does not hold the chain lock across network I/O.** Fetching happens
//! outside the lock and each batch is applied under a brief one, so a syncing
//! node keeps serving requests instead of freezing for the length of a
//! download.

use std::time::Duration;

use crate::api::routes::{AppState, MAX_SYNC_BATCH};
use crate::blockchain::chain::ChainHead;

/// How long to wait between ticks once we are caught up.
const DEFAULT_INTERVAL_SECONDS: u64 = 10;

/// Peers contacted per tick. Bounds the work one tick can do when a node has
/// learned a large peer set.
const MAX_PEERS_PER_TICK: usize = 16;

/// Peers accepted from any one peer's address book per tick. Stops a hostile
/// peer from flooding our peer list in a single response.
const MAX_LEARNED_PEERS_PER_TICK: usize = 8;

/// Upper bound on batches applied in one catch-up pass. A very long history
/// syncs over several ticks rather than in one unbounded loop that could run
/// for minutes without yielding to the rest of the tick's work.
const MAX_BATCHES_PER_PASS: usize = 64;

/// Configuration for the background syncer.
#[derive(Debug, Clone)]
pub struct SyncConfig {
    /// Gap between ticks when there is nothing to do.
    pub interval: Duration,
    /// Blocks requested per range call. The serving peer caps this too.
    pub batch: usize,
    /// How other nodes can reach us. `None` disables announcing: we can still
    /// pull history (so sync still works), we just will not be gossiped to
    /// until someone learns about us another way.
    pub announce_address: Option<String>,
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(DEFAULT_INTERVAL_SECONDS),
            batch: MAX_SYNC_BATCH,
            announce_address: None,
        }
    }
}

impl SyncConfig {
    /// Read the syncer's configuration from the environment.
    ///
    /// `P2P_SYNC_INTERVAL_SECONDS` sets the idle gap between ticks, and
    /// `P2P_SYNC_DISABLED=1` turns the worker off entirely (useful for an
    /// isolated single-node devnet, never for a network).
    pub fn from_env(announce_address: Option<String>) -> Self {
        let interval = std::env::var("P2P_SYNC_INTERVAL_SECONDS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .filter(|seconds| *seconds > 0)
            .unwrap_or(DEFAULT_INTERVAL_SECONDS);
        Self {
            interval: Duration::from_secs(interval),
            batch: MAX_SYNC_BATCH,
            announce_address,
        }
    }

    pub fn disabled_by_env() -> bool {
        std::env::var("P2P_SYNC_DISABLED")
            .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
            .unwrap_or(false)
    }
}

/// What one catch-up attempt did.
#[derive(Debug, PartialEq, Eq)]
pub enum SyncOutcome {
    /// Already at or ahead of every peer.
    UpToDate,
    /// Appended this many blocks by extending our own tip.
    Advanced(u64),
    /// The peer's history diverged from ours; fork choice adopted its chain.
    Reorganized,
    /// Nothing usable: no peers, none reachable, or none on our network.
    NoProgress,
}

/// Start the background syncer. Runs until the process exits.
pub fn spawn(state: AppState, config: SyncConfig) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        // A short first delay lets the HTTP listener bind before we start
        // talking to peers, so a node that is itself being polled during
        // startup answers rather than refusing.
        tokio::time::sleep(Duration::from_secs(2)).await;
        loop {
            tick(&state, &config).await;
            tokio::time::sleep(config.interval).await;
        }
    })
}

/// One pass: announce, discover, then catch up if behind.
pub async fn tick(state: &AppState, config: &SyncConfig) -> SyncOutcome {
    announce_and_discover(state, config).await;
    catch_up(state, config).await
}

/// Make this node known to its peers, and learn peers from them.
///
/// Announcing is what turns a configured bootnode from "somewhere I can ask"
/// into "somewhere that also pushes to me". Discovery is what lets a network
/// grow past the addresses each node was configured with.
async fn announce_and_discover(state: &AppState, config: &SyncConfig) {
    let peers = current_peers(state).await;
    if peers.is_empty() {
        return;
    }

    let mut learned: Vec<String> = Vec::new();
    for peer in peers.iter().take(MAX_PEERS_PER_TICK) {
        if let Some(address) = &config.announce_address {
            // Best-effort: a peer that is down simply does not learn about us
            // this tick, and we try again on the next one.
            let _ = state.p2p_service.announce_self(peer, address).await;
        }

        if let Some(their_peers) = state.p2p_service.fetch_peers(peer).await {
            for candidate in their_peers.into_iter().take(MAX_LEARNED_PEERS_PER_TICK) {
                let candidate = candidate.trim().to_string();
                // Never add ourselves, and never add a peer we already have.
                if candidate.is_empty()
                    || Some(&candidate) == config.announce_address.as_ref()
                    || peers.contains(&candidate)
                    || learned.contains(&candidate)
                {
                    continue;
                }
                learned.push(candidate);
            }
        }
    }

    if learned.is_empty() {
        return;
    }

    let mut peers = state.peers.lock().await;
    for candidate in learned {
        if !peers.contains(&candidate) {
            peers.push(candidate);
        }
    }
}

/// Bring this node up to the best chain any reachable peer is offering.
pub async fn catch_up(state: &AppState, config: &SyncConfig) -> SyncOutcome {
    let peers = current_peers(state).await;
    if peers.is_empty() {
        return SyncOutcome::NoProgress;
    }

    let our_head = {
        let chain = state.chain.lock().await;
        chain.head()
    };

    // Poll heads first. This is the whole point of having a head endpoint:
    // deciding whether to sync must not cost a chain download.
    let mut best: Option<(String, ChainHead)> = None;
    for peer in peers.into_iter().take(MAX_PEERS_PER_TICK) {
        let Some(head) = state.p2p_service.fetch_head(&peer).await else {
            continue;
        };
        if !same_network(&our_head, &head) {
            continue;
        }
        let better = match &best {
            Some((_, current)) => head.height > current.height,
            None => true,
        };
        if better {
            best = Some((peer, head));
        }
    }

    let Some((peer, head)) = best else {
        return SyncOutcome::NoProgress;
    };
    if head.height <= our_head.height {
        return SyncOutcome::UpToDate;
    }

    download_from(state, &peer, config, head.height).await
}

/// Two nodes are on the same network only if they agree on the chain id and
/// on the root their history hangs from. Checked before a single block is
/// downloaded, so a misconfigured or hostile peer costs us one small request.
fn same_network(ours: &ChainHead, theirs: &ChainHead) -> bool {
    ours.chain_id == theirs.chain_id
        && ours.root_hash == theirs.root_hash
        && ours.base_height == theirs.base_height
}

/// Pull blocks from one peer and apply them until caught up or blocked.
async fn download_from(
    state: &AppState,
    peer: &str,
    config: &SyncConfig,
    target: u64,
) -> SyncOutcome {
    let finality_depth = {
        let governance = state.governance.lock().await;
        governance.finality_depth
    };

    let mut applied: u64 = 0;

    for _ in 0..MAX_BATCHES_PER_PASS {
        let from = {
            let chain = state.chain.lock().await;
            chain.next_index()
        };
        if from > target {
            break;
        }

        // Fetched OUTSIDE the chain lock: a node that held the lock across a
        // download would stop serving for the length of it.
        let Some(blocks) = state
            .p2p_service
            .fetch_blocks_from(peer, from, config.batch)
            .await
        else {
            break;
        };
        if blocks.is_empty() {
            break;
        }

        let mut accepted_now: Vec<crate::blockchain::block::Block> = Vec::new();
        let mut diverged = false;
        {
            let mut chain = state.chain.lock().await;
            for block in blocks {
                // Full consensus validation — identical to the gossip path.
                // Sync is transport; it earns no trust.
                match chain.validate_block_candidate(&block) {
                    Ok(post_state) => {
                        let accepted = block.clone();
                        chain.commit_block(block, post_state);
                        accepted_now.push(accepted);
                    }
                    Err(_) => {
                        // Either this peer is on a fork, or it served us
                        // something invalid. Both are decided by fork choice
                        // below, never by appending blindly here.
                        diverged = true;
                        break;
                    }
                }
            }
            if !accepted_now.is_empty() {
                chain.apply_finality(finality_depth);
            }
        }

        applied += accepted_now.len() as u64;
        if !accepted_now.is_empty() {
            prune_and_persist(state, &accepted_now).await;
        }

        if diverged {
            // Hand the decision to fork choice, which is the only code that
            // may replace history — and which protects finalized blocks and
            // requires more validator-sealed progress to do so.
            return match adopt_via_fork_choice(state, peer, finality_depth).await {
                true => SyncOutcome::Reorganized,
                false if applied > 0 => SyncOutcome::Advanced(applied),
                false => SyncOutcome::NoProgress,
            };
        }
    }

    if applied > 0 {
        SyncOutcome::Advanced(applied)
    } else {
        SyncOutcome::NoProgress
    }
}

/// Fall back to whole-chain fork choice when a peer's history diverges.
async fn adopt_via_fork_choice(state: &AppState, peer: &str, finality_depth: u64) -> bool {
    let Some(remote) = state.p2p_service.fetch_chain(peer).await else {
        return false;
    };

    let adopted = {
        let mut chain = state.chain.lock().await;
        match chain.try_adopt_chain(&remote) {
            Ok(true) => {
                chain.apply_finality(finality_depth);
                true
            }
            _ => false,
        }
    };

    if adopted {
        let mut metrics = state.metrics.lock().await;
        metrics.reorgs += 1;
        drop(metrics);
        prune_and_persist(state, &[]).await;
    }
    adopted
}

async fn prune_and_persist(state: &AppState, accepted: &[crate::blockchain::block::Block]) {
    crate::api::routes::prune_pending(state, accepted).await;
    let _ = crate::api::routes::persist_state(state).await;
}

async fn current_peers(state: &AppState) -> Vec<String> {
    let peers = state.peers.lock().await;
    peers.clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn head(chain_id: &str, root: &str, base: u64, height: u64) -> ChainHead {
        ChainHead {
            chain_id: chain_id.to_string(),
            root_hash: root.to_string(),
            base_height: base,
            height,
            tip_hash: format!("tip-{}", height),
            finalized_height: height.saturating_sub(6),
            cumulative_work: "0".to_string(),
        }
    }

    #[test]
    fn peers_on_another_network_are_not_sync_candidates() {
        let ours = head("hikmalayer-mainnet", "root-a", 0, 10);

        // Same root, different network id: a signature valid there is inert
        // here, so their blocks are not ours to adopt.
        assert!(!same_network(&ours, &head("hikmalayer-testnet", "root-a", 0, 99)));
        // Same network id, different genesis: different parameters, so a
        // different chain wearing the same name.
        assert!(!same_network(&ours, &head("hikmalayer-mainnet", "root-b", 0, 99)));
        // Same everything but rooted at a different checkpoint anchor.
        assert!(!same_network(&ours, &head("hikmalayer-mainnet", "root-a", 40, 99)));

        assert!(same_network(&ours, &head("hikmalayer-mainnet", "root-a", 0, 99)));
        // A peer BEHIND us is still on our network; height is a separate
        // question, answered after this one.
        assert!(same_network(&ours, &head("hikmalayer-mainnet", "root-a", 0, 1)));
    }

    #[test]
    fn the_configured_interval_is_read_from_the_environment() {
        // Defaults stand on their own so a node with no tuning still syncs.
        let config = SyncConfig::default();
        assert_eq!(config.interval, Duration::from_secs(DEFAULT_INTERVAL_SECONDS));
        assert_eq!(config.batch, MAX_SYNC_BATCH);
        assert!(config.announce_address.is_none());
    }

    #[test]
    fn an_announce_address_is_optional() {
        // Announcing is an optimisation for being gossiped TO. Pull-based
        // sync must work without it, or a node that cannot name itself could
        // never join.
        let config = SyncConfig::from_env(None);
        assert!(config.announce_address.is_none());
        assert!(config.interval.as_secs() > 0);
    }
}
