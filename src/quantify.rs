use crate::discourse;
use crate::input::Input;
use crate::lens::Finding;
use crate::policy;
use crate::requirements;
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

/// Deducts points from 100, counting only CONFIRMED findings.
pub fn score(findings: &[Finding], resolved: &HashMap<String, discourse::Resolution>) -> i64 {
    let mut total = 100i64;
    for f in findings {
        if resolved.get(&f.id).map(|r| r.status.as_str()) == Some("CONFIRMED") {
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
    if policies.iter().any(|p| p.status == policy::PolicyStatus::Fail) {
        return "REQUEST_CHANGES".to_string();
    }
    if confirmed.iter().any(|f| f.severity == "P1") {
        return "COMMENT".to_string();
    }
    if let Some(reqs) = requirements {
        if reqs.iter().any(|r| r.status == "MISSING" || r.status == "AMBIGUOUS") {
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
