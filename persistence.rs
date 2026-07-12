use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

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
    let path_string = state_path();
    let path = Path::new(&path_string);
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;

    let data = serde_json::to_string_pretty(snapshot)
        .map_err(|err| std::io::Error::new(std::io::ErrorKind::Other, err))?;

    // Write to a uniquely-named temp file in the SAME directory as the
    // target. `rename` is only atomic within a single filesystem/mount
    // point, so the temp file must live alongside state.json rather than
    // in a system temp directory.
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("state.json");
    let temp_path = parent.join(format!(".{}.tmp.{}", file_name, uuid::Uuid::new_v4()));

    let write_result = (|| -> std::io::Result<()> {
        let mut file = File::create(&temp_path)?;

        // Lock permissions down before any state touches disk. No-op on
        // non-Unix targets.
        #[cfg(unix)]
        file.set_permissions(fs::Permissions::from_mode(0o600))?;

        file.write_all(data.as_bytes())?;

        // Force the temp file's contents to durable storage before it is
        // renamed into place, so a crash can't produce a rename target
        // that still has unflushed data sitting in the OS page cache.
        file.sync_all()?;
        Ok(())
    })();

    if let Err(err) = write_result {
        let _ = fs::remove_file(&temp_path);
        return Err(err);
    }

    // Atomic on POSIX systems: rename() is a single filesystem metadata
    // operation, so a crash at any point can only ever leave the old
    // state.json or the fully-written new one on disk - never a
    // half-written / corrupted file.
    if let Err(err) = fs::rename(&temp_path, path) {
        let _ = fs::remove_file(&temp_path);
        return Err(err);
    }

    // Belt-and-braces: re-assert restrictive permissions on the final
    // path in case an earlier state.json existed with looser permissions.
    #[cfg(unix)]
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;

    Ok(())
}
