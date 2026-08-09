use anyhow::{Context, Result};
use std::path::Path;

/// Normalized input. Missing information is left as None; treating it as UNKNOWN is displayed by the caller (report).
pub struct Input {
    pub content: String,
    pub content_type: String,
    /// List of (block_id, block_content) split on "### block_id" markers.
    pub blocks: Vec<(String, String)>,
    pub word_count: usize,
    pub char_count: usize,
    pub requirements: Option<String>,
    pub conventions: Option<String>,
    /// Result of checks::run_local_checks, or an externally injected deterministic check result.
    pub deterministic_results: Option<serde_json::Value>,
}

/// Splits content on "### block_id" markers. If there are no markers at all,
/// the entire content is wrapped in a single block_id="content".
pub fn parse_blocks(content: &str) -> Vec<(String, String)> {
    let mut blocks: Vec<(String, String)> = Vec::new();
    let mut current_id: Option<String> = None;
    let mut current_buf = String::new();

    for line in content.lines() {
        if let Some(rest) = line.strip_prefix("### ") {
            if let Some(id) = current_id.take() {
                blocks.push((id, current_buf.trim().to_string()));
            }
            current_id = Some(rest.trim().to_string());
            current_buf.clear();
        } else if current_id.is_some() {
            current_buf.push_str(line);
            current_buf.push('\n');
        }
    }
    if let Some(id) = current_id.take() {
        blocks.push((id, current_buf.trim().to_string()));
    }

    if blocks.is_empty() {
        blocks.push(("content".to_string(), content.trim().to_string()));
    }
    blocks
}

/// Reads and returns Some if present; None if the path itself isn't given.
fn read_opt(p: Option<&Path>) -> Result<Option<String>> {
    match p {
        None => Ok(None),
        Some(path) => {
            let s = std::fs::read_to_string(path)
                .with_context(|| format!("Failed to read file: {}", path.display()))?;
            Ok(Some(s))
        }
    }
}

pub fn normalize(
    content_path: &Path,
    content_type: &str,
    requirements_path: Option<&Path>,
    conventions_path: Option<&Path>,
    deterministic_results_path: Option<&Path>,
) -> Result<Input> {
    let content = std::fs::read_to_string(content_path)
        .with_context(|| format!("Failed to read content file: {}", content_path.display()))?;
    anyhow::ensure!(!content.trim().is_empty(), "Content is empty");

    let blocks = parse_blocks(&content);
    let word_count = content.split_whitespace().count();
    let char_count = content.chars().count();

    let requirements = read_opt(requirements_path)?;
    let conventions = read_opt(conventions_path)?;
    let deterministic_results = match deterministic_results_path {
        None => None,
        Some(p) => {
            let s = std::fs::read_to_string(p).with_context(|| {
                format!("Failed to read deterministic result file: {}", p.display())
            })?;
            Some(serde_json::from_str(&s).with_context(|| {
                format!("Failed to parse deterministic result JSON: {}", p.display())
            })?)
        }
    };

    Ok(Input {
        content,
        content_type: content_type.to_string(),
        blocks,
        word_count,
        char_count,
        requirements,
        conventions,
        deterministic_results,
    })
}
