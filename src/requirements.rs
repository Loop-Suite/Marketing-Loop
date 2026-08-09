use crate::input::Input;
use crate::lens::Finding;
use crate::llm::Llm;
use crate::promptctx::shared_context;
use crate::spec::Spec;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

pub const REQ_SYSTEM: &str = "You determine whether campaign/creative brief requirements are actually reflected in the content. \
Do not mark a requirement MET without evidence. Respond strictly in the specified JSON schema only.";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequirementCheck {
    pub requirement: String,
    pub status: String, // MET|MISSING|AMBIGUOUS|N/A
    pub evidence: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct RequirementsOutput {
    #[serde(default)]
    checks: Vec<RequirementCheck>,
}

/// Returns None if requirements (campaign/creative brief) aren't provided (nothing to verify).
pub fn verify(llm: &Llm, spec: &Spec, input: &Input, confirmed: &[&Finding]) -> Result<Option<Vec<RequirementCheck>>> {
    if input.requirements.is_none() {
        return Ok(None);
    }
    let findings_summary = confirmed
        .iter()
        .map(|f| format!("- [{}] {} — {}", f.severity, f.block_ref, f.claim))
        .collect::<Vec<_>>()
        .join("\n");
    // shared_context already includes campaign context, brand guide, requirements, and content —
    // since it's the same ctx as other calls, the OpenRouter backend can reuse the cache.
    let ctx = shared_context(spec, input);
    let task = format!(
        "# Task\nCheck each requirement against the content and render a verdict.\n\n\
         ## Confirmed findings (for reference — may serve as evidence of an unmet requirement)\n{fs}\n\n\
         ## Output (JSON only, no code fence)\n\
         {{\"checks\":[{{\"requirement\":\"requirement text verbatim\",\"status\":\"MET|MISSING|AMBIGUOUS|N/A\",\
         \"evidence\":\"block_ref evidence, or reason for missing/ambiguous\"}}]}}\n",
        fs = if findings_summary.is_empty() { "(none)".to_string() } else { findings_summary },
    );
    let v = llm.json_ctx(Some(&ctx), &task, Some(REQ_SYSTEM)).context("Requirements verification failed")?;
    let out: RequirementsOutput =
        serde_json::from_value(v).context("Requirements verification JSON schema mismatch")?;
    Ok(Some(out.checks))
}
