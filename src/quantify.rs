use crate::discourse;
use crate::input::Input;
use crate::lens::Finding;
use crate::policy;
use crate::requirements;
use crate::spec::Spec;
use std::collections::HashMap;

/// Penalty amounts. Kept as quantify.rs hardcoded values, unchanged (design-spec.md §6 — no extension allowed).
pub fn severity_penalty(s: &str) -> i64 {
    match s {
        "P0" => 25,
        "P1" => 12,
        "P2" => 5,
        "P3" => 1,
        _ => 0,
    }
}

/// Whether a finding should weigh on score/verdict: always for CONFIRMED, and for MERGED only
/// when the finding's originating lens is `tier = "blocking"` in the spec (e.g. claims_compliance).
/// A discourse CONNECT consolidates related findings — it does not dismiss them (see
/// discourse.rs's own comment on Resolution) — so a still-real blocking-tier violation must not
/// lose its scoring weight just because it was CONNECTed into another finding. Non-blocking-tier
/// MERGED findings keep the original consolidation behavior (excluded here). See issue #17.
pub fn counts_toward_score(
    f: &Finding,
    resolved: &HashMap<String, discourse::Resolution>,
    spec: &Spec,
) -> bool {
    match resolved.get(&f.id).map(|r| r.status.as_str()) {
        Some("CONFIRMED") => true,
        Some("MERGED") => spec
            .lens_by_id(&f.lens)
            .map(|l| l.tier == "blocking")
            .unwrap_or(false),
        _ => false,
    }
}

/// Deducts points from 100, counting CONFIRMED findings and blocking-tier MERGED findings (#17).
pub fn score(
    findings: &[Finding],
    resolved: &HashMap<String, discourse::Resolution>,
    spec: &Spec,
) -> i64 {
    let mut total = 100i64;
    for f in findings {
        if counts_toward_score(f, resolved, spec) {
            total -= severity_penalty(&f.severity);
        }
    }
    total.max(0)
}

/// Determines APPROVE|COMMENT|REQUEST_CHANGES|NEEDS_CONTEXT.
pub fn verdict(
    confirmed: &[&Finding],
    policies: &[policy::PolicyResult],
    requirements: &Option<Vec<requirements::RequirementCheck>>,
) -> String {
    if confirmed.iter().any(|f| f.severity == "P0") {
        return "REQUEST_CHANGES".to_string();
    }
    if policies
        .iter()
        .any(|p| p.status == policy::PolicyStatus::Fail)
    {
        return "REQUEST_CHANGES".to_string();
    }
    if confirmed.iter().any(|f| f.severity == "P1") {
        return "COMMENT".to_string();
    }
    if let Some(reqs) = requirements {
        if reqs
            .iter()
            .any(|r| r.status == "MISSING" || r.status == "AMBIGUOUS")
        {
            return "NEEDS_CONTEXT".to_string();
        }
    }
    if confirmed.is_empty() {
        "APPROVE".to_string()
    } else {
        "COMMENT".to_string()
    }
}

/// Estimated review effort (1-5) based on content size and number of selected lenses.
pub fn effort(input: &Input, lens_count: usize) -> u8 {
    let mut e: u8 = match input.word_count {
        0..=150 => 1,
        151..=400 => 2,
        401..=800 => 3,
        801..=1500 => 4,
        _ => 5,
    };
    if input.blocks.len() > 10 && e < 5 {
        e += 1;
    }
    if lens_count >= 4 && e < 5 {
        e += 1;
    }
    e.min(5)
}

/// Estimated (best, average, worst) review time in minutes, based on effort.
pub fn time_estimate(effort: u8) -> (u32, u32, u32) {
    let e = effort as u32;
    (e * 5, e * 15, e * 40)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::discourse::Resolution;
    use crate::spec::Lens;

    fn lens(id: &str, tier: &str) -> Lens {
        Lens {
            id: id.to_string(),
            title: id.to_string(),
            guide: String::new(),
            always: false,
            signal: String::new(),
            persona_name: String::new(),
            persona_voice: String::new(),
            tier: tier.to_string(),
        }
    }

    fn spec_with_lenses(lenses: Vec<Lens>) -> Spec {
        Spec {
            name: "test".to_string(),
            context: String::new(),
            lenses,
            deterministic_checks: Vec::new(),
            labels: vec!["label".to_string()],
            content_length_limit: 0,
            disclaimer_required_types: Vec::new(),
            required_brand_terms: Vec::new(),
        }
    }

    fn finding(id: &str, lens_id: &str, severity: &str) -> Finding {
        Finding {
            id: id.to_string(),
            lens: lens_id.to_string(),
            persona: "persona".to_string(),
            severity: severity.to_string(),
            label: "label".to_string(),
            block_ref: "b:0".to_string(),
            claim: "claim".to_string(),
            evidence: "evidence".to_string(),
            impact: String::new(),
            recommendation: String::new(),
        }
    }

    fn resolved(entries: &[(&str, &str)]) -> HashMap<String, Resolution> {
        entries
            .iter()
            .map(|(id, status)| {
                (
                    id.to_string(),
                    Resolution {
                        status: status.to_string(),
                        evidence: String::new(),
                    },
                )
            })
            .collect()
    }

    fn input_with_word_count(word_count: usize) -> Input {
        Input {
            content: String::new(),
            content_type: "ad_copy".to_string(),
            blocks: Vec::new(),
            word_count,
            char_count: 0,
            requirements: None,
            conventions: None,
            deterministic_results: None,
        }
    }

    #[test]
    fn severity_penalty_matches_table() {
        assert_eq!(severity_penalty("P0"), 25);
        assert_eq!(severity_penalty("P1"), 12);
        assert_eq!(severity_penalty("P2"), 5);
        assert_eq!(severity_penalty("P3"), 1);
        assert_eq!(severity_penalty("unknown"), 0);
    }

    #[test]
    fn score_deducts_only_confirmed() {
        let spec = spec_with_lenses(vec![lens("seo", "standard")]);
        let findings = vec![finding("seo-1", "seo", "P0"), finding("seo-2", "seo", "P1")];
        let resolved = resolved(&[("seo-1", "CONFIRMED"), ("seo-2", "REJECTED")]);
        assert_eq!(score(&findings, &resolved, &spec), 75); // 100 - 25
    }

    #[test]
    fn score_floors_at_zero() {
        let spec = spec_with_lenses(vec![lens("seo", "standard")]);
        let findings: Vec<Finding> = (1..=5)
            .map(|i| finding(&format!("seo-{i}"), "seo", "P0"))
            .collect();
        let entries: Vec<(&str, &str)> = findings
            .iter()
            .map(|f| (f.id.as_str(), "CONFIRMED"))
            .collect();
        let resolved = resolved(&entries);
        assert_eq!(score(&findings, &resolved, &spec), 0);
    }

    /// Regression test for #17: a blocking-tier finding (claims_compliance) merged via a
    /// discourse CONNECT must still weigh on score — CONNECT consolidates, it doesn't dismiss.
    #[test]
    fn merged_blocking_tier_finding_still_counts() {
        let spec = spec_with_lenses(vec![lens("claims_compliance", "blocking")]);
        let findings = vec![finding("claims_compliance-1", "claims_compliance", "P0")];
        let resolved = resolved(&[("claims_compliance-1", "MERGED")]);
        assert_eq!(score(&findings, &resolved, &spec), 75);
        assert!(counts_toward_score(&findings[0], &resolved, &spec));
    }

    /// Non-blocking-tier MERGED findings keep the original consolidation behavior — excluded.
    #[test]
    fn merged_standard_tier_finding_does_not_count() {
        let spec = spec_with_lenses(vec![lens("copy_craft", "standard")]);
        let findings = vec![finding("copy_craft-1", "copy_craft", "P0")];
        let resolved = resolved(&[("copy_craft-1", "MERGED")]);
        assert_eq!(score(&findings, &resolved, &spec), 100);
        assert!(!counts_toward_score(&findings[0], &resolved, &spec));
    }

    #[test]
    fn unknown_lens_is_treated_as_non_blocking() {
        // A finding referencing a lens id no longer in the spec (e.g. from a stale --prior
        // state.json after a spec change) must not panic and must default to non-blocking.
        let spec = spec_with_lenses(vec![lens("seo", "standard")]);
        let findings = vec![finding("ghost-1", "ghost_lens", "P0")];
        let resolved = resolved(&[("ghost-1", "MERGED")]);
        assert_eq!(score(&findings, &resolved, &spec), 100);
    }

    /// Guards the invariant behind #4: score computed from the pre-merge resolved map can
    /// genuinely differ from the post-merge one, which is why main.rs computes the console
    /// output and report.md from a single post-merge snapshot instead of two separate ones.
    #[test]
    fn score_before_and_after_prior_merge_can_differ() {
        let spec = spec_with_lenses(vec![lens("seo", "standard")]);
        let findings = vec![finding("prior-seo-1", "seo", "P0")];
        let pre_merge = resolved(&[]);
        let post_merge = resolved(&[("prior-seo-1", "CONFIRMED")]);
        assert_eq!(score(&findings, &pre_merge, &spec), 100);
        assert_eq!(score(&findings, &post_merge, &spec), 75);
    }

    #[test]
    fn verdict_p0_confirmed_forces_request_changes() {
        let findings = [finding("seo-1", "seo", "P0")];
        let refs: Vec<&Finding> = findings.iter().collect();
        assert_eq!(verdict(&refs, &[], &None), "REQUEST_CHANGES");
    }

    #[test]
    fn verdict_p1_confirmed_is_comment() {
        let findings = [finding("seo-1", "seo", "P1")];
        let refs: Vec<&Finding> = findings.iter().collect();
        assert_eq!(verdict(&refs, &[], &None), "COMMENT");
    }

    #[test]
    fn verdict_p2_only_is_comment() {
        let findings = [finding("seo-1", "seo", "P2")];
        let refs: Vec<&Finding> = findings.iter().collect();
        assert_eq!(verdict(&refs, &[], &None), "COMMENT");
    }

    #[test]
    fn verdict_policy_fail_forces_request_changes() {
        let policies = vec![policy::PolicyResult {
            check: "x".to_string(),
            status: policy::PolicyStatus::Fail,
            evidence: String::new(),
        }];
        assert_eq!(verdict(&[], &policies, &None), "REQUEST_CHANGES");
    }

    #[test]
    fn verdict_missing_requirement_needs_context() {
        let reqs = Some(vec![requirements::RequirementCheck {
            requirement: "r".to_string(),
            status: "MISSING".to_string(),
            evidence: String::new(),
        }]);
        assert_eq!(verdict(&[], &[], &reqs), "NEEDS_CONTEXT");
    }

    #[test]
    fn verdict_no_confirmed_is_approve() {
        assert_eq!(verdict(&[], &[], &None), "APPROVE");
    }

    #[test]
    fn effort_scales_with_word_count() {
        assert_eq!(effort(&input_with_word_count(100), 1), 1);
        assert_eq!(effort(&input_with_word_count(300), 1), 2);
        assert_eq!(effort(&input_with_word_count(600), 1), 3);
        assert_eq!(effort(&input_with_word_count(1000), 1), 4);
        assert_eq!(effort(&input_with_word_count(2000), 1), 5);
    }

    #[test]
    fn effort_caps_at_five() {
        let mut inp = input_with_word_count(2000);
        inp.blocks = (0..20).map(|i| (format!("b{i}"), String::new())).collect();
        assert_eq!(effort(&inp, 5), 5);
    }

    #[test]
    fn time_estimate_scales_linearly_with_effort() {
        assert_eq!(time_estimate(1), (5, 15, 40));
        assert_eq!(time_estimate(5), (25, 75, 200));
    }
}
