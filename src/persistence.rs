use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

use crate::{
    blockchain::{chain::Blockchain, transaction::Transaction},
    contract::contract::ContractExecutor,
    governance::GovernanceConfig,
};

const DEFAULT_STATE_PATH: &str = "data/state.json";

fn state_path() -> String {
    std::env::var("HIKMALAYER_STATE_PATH").unwrap_or_else(|_| DEFAULT_STATE_PATH.to_string())
}

/// Persisted node state. Balances, stakes, and nonces are NOT stored — they
/// are chain state, deterministically rebuilt from the blocks at startup.
#[derive(Debug, Serialize, Deserialize)]
pub struct AppSnapshot {
    pub chain: Blockchain,
    pub contracts: ContractExecutor,
    pub pending_transactions: Vec<Transaction>,
    #[serde(default)]
    pub peers: Vec<String>,
    #[serde(default)]
    pub governance: GovernanceConfig,
    #[serde(default)]
    pub slash_evidence: Vec<SlashEvidence>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlashEvidence {
    pub block_index: u64,
    pub reason: String,
    pub reporter: String,
    pub timestamp: String,
    pub slashed_amount: u64,
}

pub fn load_state() -> Option<AppSnapshot> {
    let contents = fs::read_to_string(state_path()).ok()?;
    serde_json::from_str(&contents).ok()
}

pub fn save_state(snapshot: &AppSnapshot) -> std::io::Result<()> {
    let path = state_path();
    if let Some(parent) = Path::new(&path).parent() {
        fs::create_dir_all(parent)?;
    }
    let data = serde_json::to_string_pretty(snapshot)
        .map_err(|err| std::io::Error::new(std::io::ErrorKind::Other, err))?;
    fs::write(path, data)
}
