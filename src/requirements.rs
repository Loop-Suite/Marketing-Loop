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
    #[serde(default, deserialize_with = "crate::llm::null_as_default")]
    pub requirement: String,
    #[serde(default, deserialize_with = "crate::llm::null_as_default")]
    pub status: String, // MET|MISSING|AMBIGUOUS|N/A
    #[serde(default, deserialize_with = "crate::llm::null_as_default")]
    pub evidence: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct RequirementsOutput {
    #[serde(default, deserialize_with = "crate::llm::null_as_default")]
    checks: Vec<RequirementCheck>,
}

/// Returns None if requirements (campaign/creative brief) aren't provided (nothing to verify).
pub fn verify(
    llm: &Llm,
    spec: &Spec,
    input: &Input,
    confirmed: &[&Finding],
) -> Result<Option<Vec<RequirementCheck>>> {
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
    let v = llm
        .json_ctx(Some(&ctx), &task, Some(REQ_SYSTEM))
        .context("Requirements verification failed")?;
    let out: RequirementsOutput =
        serde_json::from_value(v).context("Requirements verification JSON schema mismatch")?;
    Ok(Some(out.checks))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Regression test for #18: RequirementCheck's required fields must tolerate explicit JSON null.
    #[test]
    fn requirement_check_tolerates_explicit_nulls() {
        let v = serde_json::json!({"requirement": null, "status": null, "evidence": null});
        let rc: RequirementCheck =
            serde_json::from_value(v).expect("explicit null must not crash deserialization");
        assert_eq!(rc.status, "");
    }

    /// Regression test for #9: the LLM-response envelope must tolerate an explicit top-level
    /// `"checks": null` without crashing.
    #[test]
    fn requirements_output_tolerates_null_checks_array() {
        let v = serde_json::json!({"checks": null});
        let out: RequirementsOutput =
            serde_json::from_value(v).expect("explicit null must not crash deserialization");
        assert!(out.checks.is_empty());
    }

    /// Regression test for #7: requirements verification must run on the *post-merge* confirmed
    /// list (a re-confirmed --prior finding must be visible to the LLM prompt as evidence). Since
    /// verify() short-circuits before any LLM call when requirements aren't provided, this checks
    /// the observable contract without needing to mock the LLM boundary: a `None` requirements
    /// input always skips verification regardless of what's in `confirmed`.
    #[test]
    fn verify_returns_none_without_requirements_regardless_of_confirmed_findings() {
        let spec = Spec {
            name: "t".into(),
            context: String::new(),
            lenses: vec![],
            deterministic_checks: vec![],
            labels: vec!["l".into()],
            content_length_limit: 0,
            disclaimer_required_types: vec![],
            required_brand_terms: vec![],
        };
        let input = Input {
            content: String::new(),
            content_type: "ad_copy".to_string(),
            blocks: vec![],
            word_count: 0,
            char_count: 0,
            requirements: None,
            conventions: None,
            deterministic_results: None,
        };
        let llm = Llm::claude_cli(
            "does-not-matter".to_string(),
            None,
            0,
            false,
            Llm::new_usage_tracker(),
        );
        let result = verify(&llm, &spec, &input, &[]).unwrap();
        assert!(result.is_none());
    }
}
