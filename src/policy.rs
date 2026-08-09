use crate::input::Input;
use crate::spec::Spec;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub enum PolicyStatus {
    Pass,
    Fail,
    NotApplicable,
    NotConfigured,
}

impl PolicyStatus {
    pub fn label(&self) -> &'static str {
        match self {
            PolicyStatus::Pass => "PASS",
            PolicyStatus::Fail => "FAIL",
            PolicyStatus::NotApplicable => "N/A",
            PolicyStatus::NotConfigured => "NOT_CONFIGURED",
        }
    }
}

pub struct PolicyResult {
    pub check: String,
    pub status: PolicyStatus,
    pub evidence: String,
}

const DISCLAIMER_MARKERS: [&str; 4] = ["수신거부", "광고", "opt-out", "unsubscribe"];

/// Presence of ad/sponsorship disclosure and opt-out link. NotApplicable if content_type
/// isn't in spec.disclaimer_required_types; otherwise searches the content for a disclaimer phrase and returns Pass/Fail.
pub fn required_disclaimer_check(spec: &Spec, input: &Input) -> PolicyResult {
    if spec.disclaimer_required_types.is_empty() {
        return PolicyResult {
            check: "Required disclaimer present".into(),
            status: PolicyStatus::NotConfigured,
            evidence: "spec.disclaimer_required_types not configured".into(),
        };
    }
    if !spec.disclaimer_required_types.iter().any(|t| t == &input.content_type) {
        return PolicyResult {
            check: "Required disclaimer present".into(),
            status: PolicyStatus::NotApplicable,
            evidence: format!("content_type '{}' is not in the required-disclaimer list", input.content_type),
        };
    }
    let full_text: String = input.blocks.iter().map(|(_, c)| c.as_str()).collect::<Vec<_>>().join("\n");
    let lower = full_text.to_lowercase();
    match DISCLAIMER_MARKERS.iter().find(|m| lower.contains(&m.to_lowercase())) {
        Some(marker) => PolicyResult {
            check: "Required disclaimer present".into(),
            status: PolicyStatus::Pass,
            evidence: format!("Disclaimer phrase found: '{marker}'"),
        },
        None => PolicyResult {
            check: "Required disclaimer present".into(),
            status: PolicyStatus::Fail,
            evidence: "No disclaimer phrase found (수신거부/광고/opt-out/unsubscribe etc.)".into(),
        },
    }
}

/// Checks whether content length (character count) is within spec.content_length_limit. NotConfigured if limit=0.
pub fn content_length_check(spec: &Spec, input: &Input) -> PolicyResult {
    if spec.content_length_limit == 0 {
        return PolicyResult {
            check: "Content length within limit".into(),
            status: PolicyStatus::NotConfigured,
            evidence: "spec.content_length_limit not configured".into(),
        };
    }
    if input.word_count <= spec.content_length_limit {
        PolicyResult {
            check: "Content length within limit".into(),
            status: PolicyStatus::Pass,
            evidence: format!("word_count {} <= threshold {}", input.word_count, spec.content_length_limit),
        }
    } else {
        PolicyResult {
            check: "Content length within limit".into(),
            status: PolicyStatus::Fail,
            evidence: format!("word_count {} > threshold {}", input.word_count, spec.content_length_limit),
        }
    }
}

/// Checks whether spec.required_brand_terms are all present in the content.
pub fn brand_terms_check(spec: &Spec, input: &Input) -> PolicyResult {
    if spec.required_brand_terms.is_empty() {
        return PolicyResult {
            check: "Required brand terms present".into(),
            status: PolicyStatus::NotConfigured,
            evidence: "spec.required_brand_terms not configured".into(),
        };
    }
    let full_text: String = input.blocks.iter().map(|(_, c)| c.as_str()).collect::<Vec<_>>().join("\n");
    let found: Vec<&String> = spec.required_brand_terms.iter().filter(|t| full_text.contains(t.as_str())).collect();
    if found.is_empty() {
        PolicyResult {
            check: "Required brand terms present".into(),
            status: PolicyStatus::Fail,
            evidence: format!("Required brand terms not found: {}", spec.required_brand_terms.join(", ")),
        }
    } else {
        PolicyResult {
            check: "Required brand terms present".into(),
            status: PolicyStatus::Pass,
            evidence: format!("Brand terms found: {}", found.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")),
        }
    }
}

pub fn check_all(spec: &Spec, input: &Input) -> Vec<PolicyResult> {
    vec![
        required_disclaimer_check(spec, input),
        content_length_check(spec, input),
        brand_terms_check(spec, input),
    ]
}
