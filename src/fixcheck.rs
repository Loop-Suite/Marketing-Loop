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
/// Note (signature constraint): this scaffold's run() doesn't take Input (the raw content) as an
/// argument, so shared_context() can't be used. Only spec.context (campaign context) is used as
/// ctx — the signature is kept as finalized (no changes allowed), so if raw-content comparison is
/// needed, the higher-level design must be changed to inject the content directly into the task
/// string at the call site (not workaroundable within this file).
pub fn run(llm: &Llm, spec: &Spec, prior_confirmed: &[&Finding]) -> Result<Vec<FixResult>> {
    if prior_confirmed.is_empty() {
        return Ok(Vec::new());
    }
    let list = prior_confirmed
        .iter()
        .map(|f| format!("{} | {} | {} | {}", f.id, f.block_ref, f.claim, f.evidence))
        .collect::<Vec<_>>()
        .join("\n");
    let ctx = format!("## Campaign context\n{}\n", spec.context);
    let task = format!(
        "# Task\nDetermine whether the findings confirmed in the previous round below have been fixed in this content.\n\n\
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
