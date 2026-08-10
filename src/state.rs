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
    // --prior accepts either a previous --out directory or a direct path to its state.json.
    let joined;
    let path: &Path = if path.is_dir() {
        joined = path.join("state.json");
        &joined
    } else {
        path
    };
    let s = std::fs::read_to_string(path).with_context(|| {
        format!(
            "Failed to read {} (--prior is the state.json from a previous --out directory)",
            path.display()
        )
    })?;
    serde_json::from_str(&s).with_context(|| format!("Failed to parse {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::discourse::Resolution;
    use crate::lens::Finding;

    fn sample_state() -> State {
        let mut resolved = HashMap::new();
        resolved.insert(
            "seo-1".to_string(),
            Resolution {
                status: "CONFIRMED".to_string(),
                evidence: "evidence".to_string(),
            },
        );
        State {
            round: 0,
            findings: vec![Finding {
                id: "seo-1".to_string(),
                lens: "seo".to_string(),
                persona: "Rand".to_string(),
                severity: "P1".to_string(),
                label: "seo".to_string(),
                block_ref: "cta_1:0".to_string(),
                claim: "claim".to_string(),
                evidence: "evidence".to_string(),
                impact: String::new(),
                recommendation: String::new(),
            }],
            resolved,
        }
    }

    /// Unique per-test scratch directory under the OS temp dir — avoids clashing with other
    /// tests or parallel `cargo test` runs without adding a `tempfile` dependency.
    fn unique_tmp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "mktloop-test-state-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn write_then_load_round_trips() {
        let dir = unique_tmp_dir("roundtrip");
        let st = sample_state();
        write(&dir, &st).unwrap();
        let loaded = load(&dir.join("state.json")).unwrap();
        assert_eq!(loaded.round, st.round);
        assert_eq!(loaded.findings.len(), 1);
        assert_eq!(loaded.resolved.get("seo-1").unwrap().status, "CONFIRMED");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Regression test for #2: `--prior` given a directory (the natural --out of a previous run)
    /// must resolve to state.json inside it instead of crashing with "Is a directory".
    #[test]
    fn load_resolves_directory_path_to_state_json() {
        let dir = unique_tmp_dir("dirpath");
        write(&dir, &sample_state()).unwrap();
        let loaded = load(&dir).unwrap(); // pass the directory itself, not state.json
        assert_eq!(loaded.findings[0].id, "seo-1");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn load_missing_file_errors_instead_of_panicking() {
        let dir = unique_tmp_dir("missing");
        let result = load(&dir.join("does-not-exist.json"));
        assert!(result.is_err());
        std::fs::remove_dir_all(&dir).ok();
    }
}
