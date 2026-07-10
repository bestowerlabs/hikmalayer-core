use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

use crate::{
    blockchain::{chain::Blockchain, transaction::Transaction},
    consensus::pos::Staker,
    contract::contract::ContractExecutor,
    governance::GovernanceConfig,
    token::fungible::Token,
};

const STATE_PATH: &str = "data/state.json";

#[derive(Debug, Serialize, Deserialize)]
pub struct AppSnapshot {
    pub chain: Blockchain,
    pub token: Token,
    pub contracts: ContractExecutor,
    pub pending_transactions: Vec<Transaction>,
    pub stakers: Vec<Staker>,
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

pub fn save_state(snapshot: &AppSnapshot) -> std::io::Result<()> {
    if let Some(parent) = Path::new(STATE_PATH).parent() {
        fs::create_dir_all(parent)?;
    }
    let data = serde_json::to_string_pretty(snapshot)
        .map_err(|err| std::io::Error::new(std::io::ErrorKind::Other, err))?;

    // HM-05 fix: atomic write using temp file + rename
    // Prevents state corruption if process crashes during write
    let tmp_path = format!("{}.tmp", STATE_PATH);
    fs::write(&tmp_path, &data)?;
    fs::rename(&tmp_path, STATE_PATH)?;
    Ok(())
}
pub fn load_state() -> Option<AppSnapshot> {
    let contents = fs::read_to_string(STATE_PATH).ok()?;
    serde_json::from_str(&contents).ok()
}
