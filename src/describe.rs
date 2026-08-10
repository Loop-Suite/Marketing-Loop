use crate::input::Input;
use crate::llm::Llm;
use crate::promptctx::shared_context;
use crate::spec::Spec;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

pub const DESCRIBE_SYSTEM: &str =
    "You are a reviewer summarizing marketing content. Do not invent content that isn't there. \
Respond strictly in the specified JSON schema only.";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Describe {
    #[serde(default, deserialize_with = "crate::llm::null_as_default")]
    pub title: String,
    #[serde(default, deserialize_with = "crate::llm::null_as_default")]
    pub summary: String,
    #[serde(default, deserialize_with = "crate::llm::null_as_default")]
    pub walkthrough: String,
    #[serde(default, deserialize_with = "crate::llm::null_as_default")]
    pub labels: Vec<String>,
    #[serde(default, deserialize_with = "crate::llm::null_as_default")]
    pub can_be_split: bool,
    #[serde(default, deserialize_with = "crate::llm::null_as_default")]
    pub can_be_split_note: String,
}

pub fn run(llm: &Llm, spec: &Spec, input: &Input) -> Result<Describe> {
    let ctx = shared_context(spec, input);
    let task = "# Task\nSummarize the entire content below.\n\n\
         ## Output (JSON only, no code fence)\n\
         {\"title\":\"one line, 50 chars or fewer\",\"summary\":\"core message, target audience, tone in 2-4 sentences\",\
         \"walkthrough\":\"per-block copy summary, text separated by line breaks\",\
         \"labels\":[\"tags describing the content's nature\"],\
         \"can_be_split\":true or false (whether the material can be split by channel/target),\
         \"can_be_split_note\":\"rationale\"}\n";
    let v = llm
        .json_ctx(Some(&ctx), task, Some(DESCRIBE_SYSTEM))
        .context("describe failed")?;
    serde_json::from_value(v).context("describe JSON schema mismatch")
}

/// Deterministically scans for [TBD]/lorem ipsum/placeholder phrases (no LLM used).
pub fn todo_sections(content: &str) -> Vec<String> {
    let markers = ["[tbd]", "lorem ipsum", "placeholder"];
    content
        .lines()
        .filter(|l| {
            let low = l.to_lowercase();
            markers.iter().any(|m| low.contains(m))
        })
        .map(|l| l.trim().to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Regression test for #18: Describe's required fields (no `#[serde(default)]` at all before
    /// this fix) must tolerate an explicit JSON null the same way #9 already fixed for optional
    /// fields elsewhere.
    #[test]
    fn describe_tolerates_explicit_nulls_on_required_fields() {
        let v = serde_json::json!({
            "title": null, "summary": null, "walkthrough": null,
            "labels": null, "can_be_split": null, "can_be_split_note": null
        });
        let d: Describe =
            serde_json::from_value(v).expect("explicit null must not crash deserialization");
        assert_eq!(d.title, "");
        assert!(!d.can_be_split);
        assert!(d.labels.is_empty());
    }

    #[test]
    fn todo_sections_detects_markers_case_insensitively() {
        let content = "Line one\n[TBD] needs copy\nLorem Ipsum filler\nreal line";
        let todos = todo_sections(content);
        assert_eq!(todos.len(), 2);
    }

    #[test]
    fn todo_sections_empty_when_no_markers() {
        assert!(todo_sections("all real content here").is_empty());
    }
}
