use crate::input::Input;
use crate::llm::Llm;
use crate::promptctx::shared_context;
use crate::spec::Spec;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

pub const IMPROVE_SYSTEM: &str =
    "You are a marketing reviewer proposing concrete copy improvements. \
Don't suggest things already reflected, or trivial wording polish. \
Respond strictly in the specified JSON schema only.";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Suggestion {
    #[serde(default, deserialize_with = "crate::llm::null_as_default")]
    pub relevant_block: String,
    #[serde(default, deserialize_with = "crate::llm::null_as_default")]
    pub existing_content: String,
    #[serde(default, deserialize_with = "crate::llm::null_as_default")]
    pub suggestion_content: String,
    #[serde(default, deserialize_with = "crate::llm::null_as_default")]
    pub improved_content: String,
    #[serde(default, deserialize_with = "crate::llm::null_as_default")]
    pub one_sentence_summary: String,
    #[serde(default, deserialize_with = "crate::llm::null_as_default")]
    pub label: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct ImproveOutput {
    #[serde(default, deserialize_with = "crate::llm::null_as_default")]
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
    let v = llm
        .json_ctx(Some(&ctx), &task, Some(IMPROVE_SYSTEM))
        .context("improve failed")?;
    let out: ImproveOutput = serde_json::from_value(v).context("improve JSON schema mismatch")?;
    Ok(out.suggestions)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Regression test for #18: Suggestion's required fields must tolerate explicit JSON null.
    #[test]
    fn suggestion_tolerates_explicit_nulls() {
        let v = serde_json::json!({
            "relevant_block": null, "existing_content": null, "suggestion_content": null,
            "improved_content": null, "one_sentence_summary": null, "label": null
        });
        let s: Suggestion =
            serde_json::from_value(v).expect("explicit null must not crash deserialization");
        assert_eq!(s.label, "");
    }

    /// Regression test for #9: the LLM-response envelope must tolerate an explicit top-level
    /// `"suggestions": null` without crashing.
    #[test]
    fn improve_output_tolerates_null_suggestions_array() {
        let v = serde_json::json!({"suggestions": null});
        let out: ImproveOutput =
            serde_json::from_value(v).expect("explicit null must not crash deserialization");
        assert!(out.suggestions.is_empty());
    }
}
