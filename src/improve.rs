use crate::input::Input;
use crate::llm::Llm;
use crate::promptctx::shared_context;
use crate::spec::Spec;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

pub const IMPROVE_SYSTEM: &str = "You are a marketing reviewer proposing concrete copy improvements. \
Don't suggest things already reflected, or trivial wording polish. \
Respond strictly in the specified JSON schema only.";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Suggestion {
    pub relevant_block: String,
    pub existing_content: String,
    pub suggestion_content: String,
    pub improved_content: String,
    pub one_sentence_summary: String,
    pub label: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct ImproveOutput {
    #[serde(default)]
    suggestions: Vec<Suggestion>,
}

pub fn run(llm: &Llm, spec: &Spec, input: &Input) -> Result<Vec<Suggestion>> {
    let ctx = shared_context(spec, input);
    let task = format!(
        "# Task\nPropose concrete before/after copy rewrite suggestions for the content, per block.\n\n\
         ## Rules\n\
         - relevant_block must use the block_id exactly as it appears under '## Content by block'.\n\
         - existing_content/improved_content must quote/edit the actual content verbatim.\n\
         - one_sentence_summary must be 6 words or fewer.\n\
         - label must be exactly one of: {labels}\n\n\
         ## Output (JSON only, no code fence)\n\
         {{\"suggestions\":[{{\"relevant_block\":\"...\",\"existing_content\":\"...\",\
         \"suggestion_content\":\"...\",\"improved_content\":\"...\",\"one_sentence_summary\":\"...\",\
         \"label\":<one of the allowed values>}}]}}\n",
        labels = spec.labels_prompt(),
    );
    let v = llm.json_ctx(Some(&ctx), &task, Some(IMPROVE_SYSTEM)).context("improve failed")?;
    let out: ImproveOutput = serde_json::from_value(v).context("improve JSON schema mismatch")?;
    Ok(out.suggestions)
}
