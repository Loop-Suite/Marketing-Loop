use crate::input::Input;
use crate::llm::Llm;
use crate::promptctx::shared_context;
use crate::spec::{Lens, Spec};
use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};

/// Lenses with a persona assigned get the character identity prepended to the system prompt (suppresses sycophancy).
const LENS_SYSTEM: &str = "You are a marketing content reviewer. \
Do not turn unsubstantiated suspicion into a finding. \
Only point out problems that actually exist in the content — don't suggest mere stylistic preferences. \
Respond strictly in the specified JSON schema only.";

fn persona_system(lens: &Lens) -> String {
    if lens.persona_name.is_empty() {
        LENS_SYSTEM.to_string()
    } else {
        format!(
            "You are \"{}\". {}\nDo not agree just to agree — if your judgment differs from this identity's perspective, say so clearly.\n\n{}",
            lens.persona_name, lens.persona_voice, LENS_SYSTEM
        )
    }
}

/// An independent review finding from a lens (persona). Uses block_ref (block_id:offset) as evidence instead of file:line.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    #[serde(default)]
    pub id: String,
    pub lens: String,
    pub persona: String,
    pub severity: String, // P0-P3
    pub label: String,
    pub block_ref: String, // block_id:offset, e.g. cta_1:0
    pub claim: String,
    pub evidence: String,
    #[serde(default)]
    pub impact: String,
    #[serde(default)]
    pub recommendation: String,
}

/// Uses the LLM to select 3-5 lenses fitting the content type's nature from the candidate pool of 7.
/// Force-including `always` lenses isn't this function's responsibility (merged with spec.always_lenses() in main.rs).
pub fn select_lenses(llm: &Llm, spec: &Spec) -> Result<Vec<Lens>> {
    let optional = spec.optional_lenses();
    if optional.is_empty() {
        return Ok(Vec::new());
    }
    let catalog = optional
        .iter()
        .map(|l| {
            let who = if l.persona_name.is_empty() {
                l.title.clone()
            } else {
                format!("{} ({})", l.title, l.persona_name)
            };
            format!(
                "- id=\"{}\" | {} — selection signal: {}",
                l.id, who, l.signal
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let task = format!(
        "# Task\nReferring to the campaign context below, pick 1-3 review lenses that fit the content type and its nature (no swaps after selection).\n\n\
         ## Campaign context\n{context}\n\n\
         ## Lens candidates\n{catalog}\n\n\
         ## Output (JSON only)\n{{\"selected\":[\"id\", ...]}}\n",
        context = spec.context,
        catalog = catalog,
    );
    let v = llm
        .json(&task, Some("You are a marketing lead who only performs lens selection. Respond strictly in the JSON schema."))
        .context("Lens selection failed")?;
    let selected: Vec<String> = v
        .get("selected")
        .and_then(|s| s.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();
    let valid: Vec<Lens> = selected
        .into_iter()
        .filter_map(|id| spec.lens_by_id(&id).cloned())
        .collect();
    anyhow::ensure!(
        !valid.is_empty(),
        "Lens selection result is empty, or contains only ids not in the spec"
    );
    Ok(valid)
}

/// Intermediate struct used only for parsing the LLM response. Finding's id/lens/persona are
/// filled in server-side, so they aren't required fields here. The "id" field the LLM sends
/// alongside is automatically skipped as an unknown field.
#[derive(Debug, Deserialize)]
struct RawFinding {
    #[serde(default, deserialize_with = "crate::llm::null_as_default")]
    severity: String,
    #[serde(default, deserialize_with = "crate::llm::null_as_default")]
    label: String,
    #[serde(default, deserialize_with = "crate::llm::null_as_default")]
    claim: String,
    #[serde(default, deserialize_with = "crate::llm::null_as_default")]
    evidence: String,
    #[serde(default, deserialize_with = "crate::llm::null_as_default")]
    block_ref: String,
    #[serde(default, deserialize_with = "crate::llm::null_as_default")]
    impact: String,
    #[serde(default, deserialize_with = "crate::llm::null_as_default")]
    recommendation: String,
}

#[derive(Debug, Default, Deserialize)]
struct LensOutput {
    #[serde(default, deserialize_with = "crate::llm::null_as_default")]
    findings: Vec<RawFinding>,
}

fn build_review_task(spec: &Spec, lens: &Lens) -> String {
    format!(
        "# Task\nIndependently review the content below from the \"{title}\" perspective (do not reference other lenses' results).\n\n\
         ## This lens's focus\n{guide}\n\n\
         ## Persona perspective\n{persona_voice}\n\n\
         ## Review principles\n\
         - Every finding requires block_ref (block_id:offset, e.g. cta_1:0) as evidence.\n\
         - severity must be one of P0 (critical) through P3 (minor).\n\
         - label must be exactly one of: {labels}\n\n\
         ## Output (JSON only, no code fence)\n\
         {{\"findings\":[{{\"id\":\"...\",\"severity\":\"P0|P1|P2|P3\",\"label\":<one of the allowed values>,\
         \"claim\":\"...\",\"evidence\":\"...\",\"block_ref\":\"block_id:offset\",\"impact\":\"...\",\
         \"recommendation\":\"...\"}}]}}\n",
        title = lens.title,
        guide = lens.guide,
        persona_voice = lens.persona_voice,
        labels = spec.labels_prompt(),
    )
}

/// Independent review from a single lens (persona) perspective. Doesn't reference other lenses' results.
/// ctx is built once in review_all via shared_context(spec, input) and shared across lenses,
/// so input isn't used directly here (signature kept fixed, left as room to extend for e.g. future block-level validation).
pub fn review_lens(
    llm: &Llm,
    spec: &Spec,
    lens: &Lens,
    input: &Input,
    ctx: &str,
) -> Result<Vec<Finding>> {
    let _ = input;
    let task = build_review_task(spec, lens);
    let system = persona_system(lens);
    let v = llm
        .json_ctx(Some(ctx), &task, Some(&system))
        .with_context(|| format!("Lens review failed: {}", lens.id))?;
    let out: LensOutput = serde_json::from_value(v)
        .with_context(|| format!("Lens review JSON schema mismatch: {}", lens.id))?;
    let persona = if lens.persona_name.is_empty() {
        lens.title.clone()
    } else {
        lens.persona_name.clone()
    };
    let findings = out
        .findings
        .into_iter()
        .enumerate()
        .map(|(i, rf)| Finding {
            id: format!("{}-{}", lens.id, i + 1),
            lens: lens.id.clone(),
            persona: persona.clone(),
            severity: rf.severity,
            label: rf.label,
            block_ref: rf.block_ref,
            claim: rf.claim,
            evidence: rf.evidence,
            impact: rf.impact,
            recommendation: rf.recommendation,
        })
        .collect();
    Ok(findings)
}

/// Groups multiple lenses into threads up to `concurrency` and runs them in sequence (chunk-wise barrier). Counterpart to the original main.rs::par_map.
/// ctx (campaign context, brand guide, requirements, content) is identical for every lens, so it's built once and shared.
pub fn review_all(
    llm: &Llm,
    spec: &Spec,
    lenses: &[Lens],
    input: &Input,
    concurrency: usize,
) -> Result<Vec<Finding>> {
    let ctx = shared_context(spec, input);
    let c = concurrency.max(1);
    let mut out: Vec<Finding> = Vec::new();
    let mut rest: Vec<&Lens> = lenses.iter().collect();
    while !rest.is_empty() {
        let take = c.min(rest.len());
        let chunk: Vec<&Lens> = rest.drain(..take).collect();
        let results: Vec<Result<Vec<Finding>>> = std::thread::scope(|s| {
            let handles: Vec<_> = chunk
                .into_iter()
                .map(|lens| s.spawn(|| review_lens(llm, spec, lens, input, &ctx)))
                .collect();
            handles
                .into_iter()
                .map(|h| {
                    h.join()
                        .map_err(|_| anyhow!("worker thread panicked"))
                        .and_then(|r| r)
                })
                .collect()
        });
        for r in results {
            out.extend(r?);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Regression test for #9/#18: explicit JSON null on required RawFinding fields must not
    /// crash deserialization.
    #[test]
    fn raw_finding_tolerates_explicit_nulls() {
        let v = serde_json::json!({
            "severity": null, "label": null, "claim": null, "evidence": null,
            "block_ref": null, "impact": null, "recommendation": null
        });
        let rf: RawFinding =
            serde_json::from_value(v).expect("explicit null must not crash deserialization");
        assert_eq!(rf.severity, "");
        assert_eq!(rf.block_ref, "");
    }

    #[test]
    fn lens_output_tolerates_null_findings_array() {
        let v = serde_json::json!({"findings": null});
        let out: LensOutput =
            serde_json::from_value(v).expect("explicit null must not crash deserialization");
        assert!(out.findings.is_empty());
    }

    #[test]
    fn select_lenses_returns_empty_when_no_optional_lenses() {
        let spec = Spec {
            name: "t".into(),
            context: String::new(),
            lenses: vec![Lens {
                id: "claims_compliance".into(),
                title: "Claims".into(),
                guide: String::new(),
                always: true,
                signal: String::new(),
                persona_name: String::new(),
                persona_voice: String::new(),
                tier: "blocking".into(),
            }],
            deterministic_checks: vec![],
            labels: vec!["l".into()],
            content_length_limit: 0,
            disclaimer_required_types: vec![],
            required_brand_terms: vec![],
        };
        // Every lens is `always = true`, so optional_lenses() is empty and select_lenses must
        // short-circuit without needing an LLM call.
        let llm = Llm::claude_cli(
            "does-not-matter".to_string(),
            None,
            0,
            false,
            Llm::new_usage_tracker(),
        );
        let selected = select_lenses(&llm, &spec).unwrap();
        assert!(selected.is_empty());
    }
}
