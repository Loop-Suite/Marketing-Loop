use crate::input::Input;
use crate::spec::Spec;

/// Counterpart to the original semgrep.rs (local deterministic checks). Computed with pure Rust
/// string matching, no external tools. Covers only the "self-implemented" subset of
/// design-spec.md §3 deterministic_checks (banned_words/legal_claim_scan/
/// readability_score/brand_keyword_match/trademark_symbol_check) — the rest (spelling, links,
/// accessibility, etc.) needs existing external tool integration and isn't handled at this stage
/// (can be injected separately via deterministic_results on input).
pub fn run_local_checks(input: &Input, spec: &Spec) -> serde_json::Value {
    let (bw_status, bw_evidence) = banned_words(&input.content);
    let (lc_status, lc_evidence) = legal_claim_scan(&input.content);
    let (rs_status, rs_evidence) = readability_score(&input.content);
    let (bk_status, bk_evidence) = brand_keyword_match(&input.content, &spec.required_brand_terms);
    let (ts_status, ts_evidence) =
        trademark_symbol_check(&input.content, &spec.required_brand_terms);

    serde_json::json!({
        "banned_words": {"status": bw_status, "evidence": bw_evidence},
        "legal_claim_scan": {"status": lc_status, "evidence": lc_evidence},
        "readability_score": {"status": rs_status, "evidence": rs_evidence},
        "brand_keyword_match": {"status": bk_status, "evidence": bk_evidence},
        "trademark_symbol_check": {"status": ts_status, "evidence": ts_evidence},
    })
}

/// Banned-word / exaggerated-superlative scan. Assumption: the word list is example-level only
/// (should be split out into a spec-extensible form for real operation; uncertain).
/// NOTE: the word list below is intentionally kept in Korean — these are the literal Korean
/// phrases the check matches against real-world Korean-language ad copy, not translatable text.
fn banned_words(content: &str) -> (&'static str, String) {
    const BANNED: &[&str] = &["최고", "무조건", "100% 보장", "업계 1위", "완벽한", "절대"];
    let hits: Vec<&str> = BANNED
        .iter()
        .copied()
        .filter(|w| content.contains(w))
        .collect();
    if hits.is_empty() {
        (
            "PASS",
            "No banned words/exaggerated superlatives detected".to_string(),
        )
    } else {
        (
            "FAIL",
            format!("Banned words detected: {}", hits.join(", ")),
        )
    }
}

/// Detects absolute/medical/financial efficacy claims.
/// NOTE: the phrase list below is intentionally kept in Korean — these are the literal Korean
/// phrases the check matches against real-world Korean-language ad copy, not translatable text.
fn legal_claim_scan(content: &str) -> (&'static str, String) {
    const CLAIMS: &[&str] = &[
        "완치",
        "치료 효과",
        "부작용 없음",
        "원금 보장",
        "고수익 보장",
        "확실한 효과",
    ];
    let hits: Vec<&str> = CLAIMS
        .iter()
        .copied()
        .filter(|w| content.contains(w))
        .collect();
    if hits.is_empty() {
        (
            "PASS",
            "No absolute medical/financial efficacy claims detected".to_string(),
        )
    } else {
        (
            "FAIL",
            format!(
                "Absolute efficacy/financial claims detected: {}",
                hits.join(", ")
            ),
        )
    }
}

/// Assumption: not a formal formula like Flesch, but an "average words per sentence" approximation
/// (uses only split_whitespace, no syllable counting) — a simple proxy metric to compensate for
/// the lack of a dedicated tool like textstat; precision is low (uncertain).
fn readability_score(content: &str) -> (&'static str, String) {
    let word_count = content.split_whitespace().count();
    let sentence_count = content
        .split(|c| c == '.' || c == '!' || c == '?' || c == '\n')
        .filter(|s| !s.trim().is_empty())
        .count()
        .max(1);
    let avg = word_count as f64 / sentence_count as f64;
    let (status, note) = if avg <= 20.0 {
        ("PASS", "Average words per sentence is good")
    } else if avg <= 30.0 {
        ("WARN", "Average words per sentence is somewhat long")
    } else {
        (
            "FAIL",
            "Average words per sentence is excessive — readability may suffer",
        )
    };
    (status, format!("{note} (avg {avg:.1} words/sentence, {word_count} words / {sentence_count} sentences total)"))
}

/// Checks whether spec.required_brand_terms are all present in the content.
fn brand_keyword_match(content: &str, required: &[String]) -> (&'static str, String) {
    if required.is_empty() {
        return (
            "PASS",
            "spec.required_brand_terms not configured".to_string(),
        );
    }
    let missing: Vec<&str> = required
        .iter()
        .map(|s| s.as_str())
        .filter(|t| !content.contains(t))
        .collect();
    if missing.is_empty() {
        ("PASS", "All required brand keywords present".to_string())
    } else {
        (
            "FAIL",
            format!("Missing required brand keywords: {}", missing.join(", ")),
        )
    }
}

/// WARN if required brand keywords are configured but no ®/™ mark appears anywhere in the content.
/// Assumption: doesn't verify which brand term the mark should be attached to (position-agnostic, presence only).
fn trademark_symbol_check(content: &str, required: &[String]) -> (&'static str, String) {
    if required.is_empty() {
        return (
            "PASS",
            "spec.required_brand_terms not configured".to_string(),
        );
    }
    if content.contains('®') || content.contains('™') {
        ("PASS", "®/™ mark confirmed".to_string())
    } else {
        (
            "WARN",
            "No ®/™ mark on brand/product name — please check".to_string(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn banned_words_detects_hits() {
        let (status, evidence) = banned_words("이 제품은 업계 1위 입니다");
        assert_eq!(status, "FAIL");
        assert!(evidence.contains("업계 1위"));
    }

    #[test]
    fn banned_words_passes_clean_copy() {
        let (status, _) = banned_words("이 제품은 좋습니다");
        assert_eq!(status, "PASS");
    }

    #[test]
    fn legal_claim_scan_detects_absolute_claims() {
        let (status, evidence) = legal_claim_scan("이 약은 완치 효과가 있습니다");
        assert_eq!(status, "FAIL");
        assert!(evidence.contains("완치"));
    }

    #[test]
    fn legal_claim_scan_passes_clean_copy() {
        let (status, _) = legal_claim_scan("이 제품은 도움이 될 수 있습니다");
        assert_eq!(status, "PASS");
    }

    #[test]
    fn readability_score_pass_warn_fail_bands() {
        let words = |n: usize| (0..n).map(|_| "word").collect::<Vec<_>>().join(" ");
        assert_eq!(readability_score(&format!("{}.", words(15))).0, "PASS"); // avg 15
        assert_eq!(readability_score(&format!("{}.", words(25))).0, "WARN"); // avg 25
        assert_eq!(readability_score(&format!("{}.", words(35))).0, "FAIL"); // avg 35
    }

    #[test]
    fn brand_keyword_match_requires_all_terms() {
        let required = vec!["Acme".to_string(), "Widget".to_string()];
        assert_eq!(
            brand_keyword_match("Acme Widget is great", &required).0,
            "PASS"
        );
        let (status, evidence) = brand_keyword_match("Acme is great", &required);
        assert_eq!(status, "FAIL");
        assert!(evidence.contains("Widget"));
    }

    #[test]
    fn brand_keyword_match_passes_when_unconfigured() {
        assert_eq!(brand_keyword_match("anything", &[]).0, "PASS");
    }

    #[test]
    fn trademark_symbol_check_warns_without_mark_then_passes_with_it() {
        let required = vec!["Acme".to_string()];
        assert_eq!(trademark_symbol_check("Acme is great", &required).0, "WARN");
        assert_eq!(
            trademark_symbol_check("Acme® is great", &required).0,
            "PASS"
        );
    }

    #[test]
    fn run_local_checks_assembles_all_five_checks() {
        let spec = Spec {
            name: "t".into(),
            context: String::new(),
            lenses: vec![],
            deterministic_checks: vec![],
            labels: vec!["l".into()],
            content_length_limit: 0,
            disclaimer_required_types: vec![],
            required_brand_terms: vec!["Acme".to_string()],
        };
        let input = Input {
            content: "Acme is great".to_string(),
            content_type: "ad_copy".to_string(),
            blocks: vec![],
            word_count: 3,
            char_count: 14,
            requirements: None,
            conventions: None,
            deterministic_results: None,
        };
        let v = run_local_checks(&input, &spec);
        for key in [
            "banned_words",
            "legal_claim_scan",
            "readability_score",
            "brand_keyword_match",
            "trademark_symbol_check",
        ] {
            assert!(v.get(key).is_some(), "missing key: {key}");
        }
    }
}
