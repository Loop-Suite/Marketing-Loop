use crate::discourse::Resolution;
use crate::lens::Finding;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// Snapshot of findings and verdicts at the end of a round. The next round (--prior) picks up from here.
/// Same 3-field structure as the original state.rs (design-spec.md §6 — the actual extended schema
/// is handled separately at the report/state assembly stage).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct State {
    pub round: usize,
    pub findings: Vec<Finding>,
    pub resolved: HashMap<String, Resolution>,
}

pub fn write(out_dir: &Path, state: &State) -> Result<()> {
    let path = out_dir.join("state.json");
    std::fs::write(&path, serde_json::to_string_pretty(state)?)
        .with_context(|| format!("Failed to write {}", path.display()))?;
    Ok(())
}

pub fn load(path: &Path) -> Result<State> {
    let s = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read {} (--prior is the state.json from a previous --out directory)", path.display()))?;
    serde_json::from_str(&s).with_context(|| format!("Failed to parse {}", path.display()))
}
