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
    if !spec
        .disclaimer_required_types
        .iter()
        .any(|t| t == &input.content_type)
    {
        return PolicyResult {
            check: "Required disclaimer present".into(),
            status: PolicyStatus::NotApplicable,
            evidence: format!(
                "content_type '{}' is not in the required-disclaimer list",
                input.content_type
            ),
        };
    }
    let full_text: String = input
        .blocks
        .iter()
        .map(|(_, c)| c.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let lower = full_text.to_lowercase();
    match DISCLAIMER_MARKERS
        .iter()
        .find(|m| lower.contains(&m.to_lowercase()))
    {
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
    if input.char_count <= spec.content_length_limit {
        PolicyResult {
            check: "Content length within limit".into(),
            status: PolicyStatus::Pass,
            evidence: format!(
                "char_count {} <= threshold {}",
                input.char_count, spec.content_length_limit
            ),
        }
    } else {
        PolicyResult {
            check: "Content length within limit".into(),
            status: PolicyStatus::Fail,
            evidence: format!(
                "char_count {} > threshold {}",
                input.char_count, spec.content_length_limit
            ),
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
    let full_text: String = input
        .blocks
        .iter()
        .map(|(_, c)| c.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let found: Vec<&String> = spec
        .required_brand_terms
        .iter()
        .filter(|t| full_text.contains(t.as_str()))
        .collect();
    if found.is_empty() {
        PolicyResult {
            check: "Required brand terms present".into(),
            status: PolicyStatus::Fail,
            evidence: format!(
                "Required brand terms not found: {}",
                spec.required_brand_terms.join(", ")
            ),
        }
    } else {
        PolicyResult {
            check: "Required brand terms present".into(),
            status: PolicyStatus::Pass,
            evidence: format!(
                "Brand terms found: {}",
                found
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::Spec;

    fn spec(
        content_length_limit: usize,
        disclaimer_types: Vec<String>,
        brand_terms: Vec<String>,
    ) -> Spec {
        Spec {
            name: "t".into(),
            context: String::new(),
            lenses: vec![],
            deterministic_checks: vec![],
            labels: vec!["l".into()],
            content_length_limit,
            disclaimer_required_types: disclaimer_types,
            required_brand_terms: brand_terms,
        }
    }

    fn input(content_type: &str, char_count: usize, blocks: Vec<(&str, &str)>) -> Input {
        Input {
            content: String::new(),
            content_type: content_type.to_string(),
            blocks: blocks
                .into_iter()
                .map(|(a, b)| (a.to_string(), b.to_string()))
                .collect(),
            word_count: 0,
            char_count,
            requirements: None,
            conventions: None,
            deterministic_results: None,
        }
    }

    #[test]
    fn content_length_check_not_configured_when_limit_zero() {
        let sp = spec(0, vec![], vec![]);
        let inp = input("email", 100, vec![]);
        assert_eq!(
            content_length_check(&sp, &inp).status,
            PolicyStatus::NotConfigured
        );
    }

    /// Regression test for #5: this check must compare against char_count, not word_count — a
    /// body whose char_count already exceeds the character-based limit must FAIL.
    #[test]
    fn content_length_check_compares_char_count_not_word_count() {
        let sp = spec(10, vec![], vec![]);
        let inp = input("email", 50, vec![]); // char_count 50 > limit 10
        assert_eq!(content_length_check(&sp, &inp).status, PolicyStatus::Fail);
    }

    #[test]
    fn content_length_check_passes_within_limit() {
        let sp = spec(100, vec![], vec![]);
        let inp = input("email", 50, vec![]);
        assert_eq!(content_length_check(&sp, &inp).status, PolicyStatus::Pass);
    }

    #[test]
    fn required_disclaimer_check_not_configured_when_unset() {
        let sp = spec(0, vec![], vec![]);
        let inp = input("email", 0, vec![("b", "buy now")]);
        assert_eq!(
            required_disclaimer_check(&sp, &inp).status,
            PolicyStatus::NotConfigured
        );
    }

    #[test]
    fn required_disclaimer_check_not_applicable_for_other_type() {
        let sp = spec(0, vec!["email".to_string()], vec![]);
        let inp = input("social_post", 0, vec![("b", "buy now")]);
        assert_eq!(
            required_disclaimer_check(&sp, &inp).status,
            PolicyStatus::NotApplicable
        );
    }

    #[test]
    fn required_disclaimer_check_fails_without_marker() {
        let sp = spec(0, vec!["email".to_string()], vec![]);
        let inp = input("email", 0, vec![("b", "buy now")]);
        assert_eq!(
            required_disclaimer_check(&sp, &inp).status,
            PolicyStatus::Fail
        );
    }

    #[test]
    fn required_disclaimer_check_passes_with_marker() {
        let sp = spec(0, vec!["email".to_string()], vec![]);
        let inp = input("email", 0, vec![("b", "buy now. unsubscribe here")]);
        assert_eq!(
            required_disclaimer_check(&sp, &inp).status,
            PolicyStatus::Pass
        );
    }

    #[test]
    fn brand_terms_check_not_configured_when_unset() {
        let sp = spec(0, vec![], vec![]);
        let inp = input("email", 0, vec![("b", "buy now")]);
        assert_eq!(
            brand_terms_check(&sp, &inp).status,
            PolicyStatus::NotConfigured
        );
    }

    #[test]
    fn brand_terms_check_fails_when_missing() {
        let sp = spec(0, vec![], vec!["Acme".to_string()]);
        let inp = input("email", 0, vec![("b", "buy now")]);
        assert_eq!(brand_terms_check(&sp, &inp).status, PolicyStatus::Fail);
    }

    #[test]
    fn brand_terms_check_passes_when_present() {
        let sp = spec(0, vec![], vec!["Acme".to_string()]);
        let inp = input("email", 0, vec![("b", "Acme is here")]);
        assert_eq!(brand_terms_check(&sp, &inp).status, PolicyStatus::Pass);
    }

    #[test]
    fn check_all_returns_three_results() {
        let sp = spec(0, vec![], vec![]);
        let inp = input("email", 0, vec![]);
        assert_eq!(check_all(&sp, &inp).len(), 3);
    }
}
