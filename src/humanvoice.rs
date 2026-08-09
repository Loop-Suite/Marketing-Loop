use crate::input::Input;
use crate::lens::Finding;
use crate::llm::Llm;
use crate::promptctx::shared_context;
use crate::spec::Spec;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

pub const HUMANVOICE_SYSTEM: &str = "You are a reviewer leaving comments on marketing content as if written directly by a human. \
Mark minor nitpicks with 'Nit:', and write politely, mixing in questions rather than only flat assertions. \
Do not invent new points that aren't in the confirmed list.";

/// A concrete good practice worth preserving.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoodThing {
    pub block_ref: String,
    pub practice: String,
    pub why: String,
}

/// Rewrites confirmed findings and good things in the tone of a human-written review comment.
pub fn rewrite(llm: &Llm, spec: &Spec, input: &Input, confirmed: &[&Finding], good_things: &[GoodThing]) -> Result<String> {
    if confirmed.is_empty() && good_things.is_empty() {
        return Ok("(No confirmed findings or good things — skipping human-voice rewrite)".to_string());
    }
    let findings_text = confirmed
        .iter()
        .map(|f| format!("- [{}] {} {} (evidence: {})", f.severity, f.block_ref, f.claim, f.evidence))
        .collect::<Vec<_>>()
        .join("\n");
    let good_text = good_things
        .iter()
        .map(|g| format!("- {} — {} ({})", g.block_ref, g.practice, g.why))
        .collect::<Vec<_>>()
        .join("\n");
    let ctx = shared_context(spec, input);
    let task = format!(
        "# Task\nRewrite the confirmed review results below in the tone of a human-written review comment.\n\n\
         ## Confirmed findings\n{findings_text}\n\n## Good things\n{good_text}\n\n\
         ## Output rules\n\
         - Output only the comment body (no meta-commentary or preamble).\n\
         - Start minor nitpicks with 'Nit:'.\n\
         - Prefer polite phrasing mixed with questions over flat assertions.\n\
         - Do not invent new points not in the list above — rephrase only.\n",
        findings_text = if findings_text.is_empty() { "(none)".to_string() } else { findings_text },
        good_text = if good_text.is_empty() { "(none)".to_string() } else { good_text },
    );
    llm.text_ctx(Some(&ctx), &task, Some(HUMANVOICE_SYSTEM)).context("human-voice rewrite failed")
}
