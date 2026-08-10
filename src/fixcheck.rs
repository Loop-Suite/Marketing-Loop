use crate::lens::Finding;
use crate::llm::Llm;
use crate::spec::Spec;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

pub const FIXCHECK_SYSTEM: &str = "You determine whether findings confirmed in a previous round have \
actually been fixed in this content. Do not mark FIXED without evidence. If it can't be verified, use UNKNOWN. \
Respond strictly in the specified JSON schema only.";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FixResult {
    pub finding_id: String,
    pub status: String, // FIXED|STILL_OPEN|UNKNOWN
    pub evidence: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct FixCheckOutput {
    #[serde(default)]
    results: Vec<FixResult>,
}

/// Determines whether findings confirmed in the previous round (--prior) were actually fixed in this content.
/// If prior_confirmed is empty, returns an empty result (round 1, or no prior confirmed findings) — LLM call is skipped.
///
/// `content` is the current (revised) content being reviewed this round — it must be included in
/// the prompt so the LLM can actually check whether each prior finding was fixed, instead of
/// judging FIXED/STILL_OPEN/UNKNOWN from the finding list and campaign context alone.
pub fn run(llm: &Llm, spec: &Spec, content: &str, prior_confirmed: &[&Finding]) -> Result<Vec<FixResult>> {
    if prior_confirmed.is_empty() {
        return Ok(Vec::new());
    }
    let list = prior_confirmed
        .iter()
        .map(|f| format!("{} | {} | {} | {}", f.id, f.block_ref, f.claim, f.evidence))
        .collect::<Vec<_>>()
        .join("\n");
    let ctx = format!(
        "## Campaign context\n{}\n\n## Current content (this round)\n{}\n",
        spec.context, content
    );
    let task = format!(
        "# Task\nDetermine whether the findings confirmed in the previous round below have been fixed in the current content above.\n\n\
         ## Findings confirmed in the previous round (id | block_ref | claim | evidence)\n{list}\n\n\
         ## Output (JSON only, no code fence)\n\
         {{\"results\":[{{\"finding_id\":\"...\",\"status\":\"FIXED|STILL_OPEN|UNKNOWN\",\"evidence\":\"...\"}}]}}\n",
        list = list
    );
    let v = llm
        .json_ctx(Some(&ctx), &task, Some(FIXCHECK_SYSTEM))
        .context("fix check failed")?;
    let out: FixCheckOutput =
        serde_json::from_value(v).context("fix check JSON schema mismatch")?;
    Ok(out.results)
}
