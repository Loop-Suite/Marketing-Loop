use crate::lens::Finding;
use crate::llm::Llm;
use crate::spec::Spec;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub const DISCOURSE_SYSTEM: &str = "You are a marketing review panel that cross-validates findings from multiple personas. \
Do not agree or rebut without substance. Use AGREE only when citing new evidence (new_evidence). \
This round must include at least one CHALLENGE. \
Respond strictly in the specified JSON schema only.";

/// Ported as-is from the original discourse.rs's AGREE/CHALLENGE/CONNECT/SURFACE structure.
/// Reviewer identity (persona) is never exposed — judged solely by target_finding_id/evidence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Move {
    #[serde(rename = "move")]
    pub kind: String, // AGREE|CHALLENGE|CONNECT|SURFACE
    pub target_finding_id: String,
    #[serde(default)]
    pub new_evidence: String,
    #[serde(default)]
    pub detail: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DiscourseRound {
    pub round: usize,
    #[serde(default)]
    pub moves: Vec<Move>,
}

/// status: CONFIRMED|REJECTED|MERGED|UNCERTAIN — vocabulary matched to the original report.rs's
/// findings filter condition (only CONFIRMED is exposed). evidence holds the basis for the verdict
/// (including rejection reason, merge target, etc.).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Resolution {
    pub status: String,
    pub evidence: String,
}

/// Reviewer (lens/persona) identity is never exposed — only id/block_ref/claim/evidence/severity/label are listed (anonymized).
fn findings_catalog(findings: &[Finding]) -> String {
    findings
        .iter()
        .map(|f| {
            format!(
                "- id={} | block_ref={} | severity={} | label={}\n  claim: {}\n  evidence: {}",
                f.id, f.block_ref, f.severity, f.label, f.claim, f.evidence
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn build_round_prompt(spec: &Spec, findings: &[Finding], round: usize) -> String {
    format!(
        "# Task\nPerform round {round} discourse. Findings from all lenses have been disclosed (reviewer identity is withheld).\n\n\
         ## Campaign context\n{context}\n\n\
         ## All findings\n{catalog}\n\n\
         ## Rules\n\
         - Each move is one of AGREE/CHALLENGE/CONNECT/SURFACE; specify the target finding id in target_finding_id.\n\
         - AGREE: only when there is new evidence not already in the target finding. new_evidence must be filled with that evidence.\n\
         - CHALLENGE: at least once this round. Rebut specifically in detail using one of evidence/counterexample/scope/severity/assumption.\n\
         - CONNECT: name two or more related finding ids in detail and describe the cause/effect relationship.\n\
         - SURFACE: describe a newly discovered issue in detail along with evidence.\n\
         - Do not produce agreement/rebuttal without substance.\n\n\
         ## Output (JSON only, no code fence)\n\
         {{\"moves\":[{{\"move\":\"AGREE|CHALLENGE|CONNECT|SURFACE\",\"target_finding_id\":\"...\",\
         \"new_evidence\":\"...\",\"detail\":\"...\"}}]}}\n",
        round = round,
        context = spec.context,
        catalog = findings_catalog(findings),
    )
}

#[derive(Debug, Default, Deserialize)]
struct MovesResponse {
    #[serde(default)]
    moves: Vec<Move>,
}

fn run_round_call(llm: &Llm, spec: &Spec, findings: &[Finding], round: usize) -> Result<DiscourseRound> {
    let prompt = build_round_prompt(spec, findings, round);
    let v = llm
        .json(&prompt, Some(DISCOURSE_SYSTEM))
        .with_context(|| format!("discourse round {round} failed"))?;
    let mr: MovesResponse =
        serde_json::from_value(v).with_context(|| format!("discourse round {round} JSON schema mismatch"))?;
    Ok(DiscourseRound { round, moves: mr.moves })
}

/// Computes the final Resolution per finding_id, factoring in only moves that weren't invalidated.
/// Priority: valid CHALLENGE (unrebutted) > CONNECT > only valid AGREE present > default UNCERTAIN.
fn resolve_findings(findings: &[Finding], rounds: &[DiscourseRound]) -> HashMap<String, Resolution> {
    let all_moves: Vec<&Move> = rounds.iter().flat_map(|r| r.moves.iter()).collect();

    findings
        .iter()
        .map(|f| {
            let valid_challenges: Vec<&&Move> = all_moves
                .iter()
                .filter(|m| m.kind == "CHALLENGE" && m.target_finding_id == f.id && validate_challenge_on_legal(m, findings))
                .collect();
            let connects: Vec<&&Move> = all_moves
                .iter()
                .filter(|m| m.kind == "CONNECT" && m.target_finding_id == f.id)
                .collect();
            let valid_agrees: Vec<&&Move> = all_moves
                .iter()
                .filter(|m| m.kind == "AGREE" && m.target_finding_id == f.id && validate_agree(m))
                .collect();

            let resolution = if !valid_challenges.is_empty() {
                let ev = valid_challenges.iter().map(|m| m.detail.as_str()).collect::<Vec<_>>().join(" / ");
                Resolution { status: "REJECTED".to_string(), evidence: ev }
            } else if !connects.is_empty() {
                // CONNECT does not replace the original finding — it stays in the findings list, only status becomes MERGED.
                let ev = connects.iter().map(|m| m.detail.as_str()).collect::<Vec<_>>().join(" / ");
                Resolution { status: "MERGED".to_string(), evidence: ev }
            } else if !valid_agrees.is_empty() {
                let ev = valid_agrees.iter().map(|m| m.new_evidence.as_str()).collect::<Vec<_>>().join(" / ");
                Resolution { status: "CONFIRMED".to_string(), evidence: ev }
            } else {
                Resolution { status: "UNCERTAIN".to_string(), evidence: "No basis for a verdict found in the cross-validation round".to_string() }
            };
            (f.id.clone(), resolution)
        })
        .collect()
}

/// Runs discourse rounds up to max_rounds, resolving findings into CONFIRMED/REJECTED/MERGED/UNCERTAIN.
/// If a round has no valid CHALLENGE (legal targets require a regulation citation), re-request once —
/// if the re-request also has none, just let it pass.
pub fn run(
    llm: &Llm,
    spec: &Spec,
    findings: &[Finding],
    max_rounds: usize,
) -> Result<(Vec<DiscourseRound>, HashMap<String, Resolution>)> {
    if findings.is_empty() {
        return Ok((Vec::new(), HashMap::new()));
    }
    let max_rounds = max_rounds.max(1);
    let mut rounds: Vec<DiscourseRound> = Vec::new();

    for round in 1..=max_rounds {
        let mut dr = run_round_call(llm, spec, findings, round)?;
        let has_valid_challenge = dr
            .moves
            .iter()
            .any(|m| m.kind == "CHALLENGE" && validate_challenge_on_legal(m, findings));
        if !has_valid_challenge {
            dr = run_round_call(llm, spec, findings, round).context("re-request for missing CHALLENGE failed")?;
            // If the re-request result also has no valid CHALLENGE, let it pass anyway (same as the original rule).
        }
        rounds.push(dr);
    }

    let resolved = resolve_findings(findings, &rounds);
    Ok((rounds, resolved))
}

/// AGREE is valid only when there is new evidence (new_evidence) not already in the target finding.
/// An AGREE with empty new_evidence is treated as invalid (still recorded in discourse_rounds, but not reflected in the verdict).
pub fn validate_agree(mv: &Move) -> bool {
    mv.kind != "AGREE" || !mv.new_evidence.trim().is_empty()
}

/// Determines whether an evidence string cites a regulatory basis such as policy/data/brand guide.
/// NOTE: this intentionally still matches literal Korean legal-citation markers — the domain this
/// tool operates in (Korean marketing compliance) requires detecting Korean-language regulation
/// citations, so these patterns are not translated. Treated as a citation if the string contains
/// "규정" ("regulation") or "§", or if a digit is immediately followed (spaces allowed) by "조"
/// (Article) or "항" (Clause/Paragraph) — e.g. "3조", "제17조" (Article 17), "2항" (Clause 2).
pub fn is_regulation_citation(evidence: &str) -> bool {
    if evidence.contains("규정") || evidence.contains('§') {
        return true;
    }
    let chars: Vec<char> = evidence.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i].is_ascii_digit() {
            let mut j = i;
            while j < chars.len() && chars[j].is_ascii_digit() {
                j += 1;
            }
            let mut k = j;
            while k < chars.len() && chars[k] == ' ' {
                k += 1;
            }
            if k < chars.len() && (chars[k] == '조' || chars[k] == '항') {
                return true;
            }
            i = j;
        } else {
            i += 1;
        }
    }
    false
}

/// A CHALLENGE against a claims_compliance (legal) lens finding is accepted only when
/// evidence-based; a purely stylistic rebuttal (e.g. tone preference) is not accepted.
/// If the target isn't a legal-natured finding, this rule doesn't apply (always valid).
///
/// "legal" determination: besides target_finding_id itself starting with "legal-" (a string
/// convention), this codebase's actual legal lens id is "claims_compliance" and its label is
/// "legal_compliance", so a target found in findings with lens=="claims_compliance" or a label
/// starting with "legal" is also treated as legal.
pub fn validate_challenge_on_legal(mv: &Move, findings: &[Finding]) -> bool {
    if mv.kind != "CHALLENGE" {
        return true;
    }
    let target = findings.iter().find(|f| f.id == mv.target_finding_id);
    let is_legal_target = mv.target_finding_id.starts_with("legal-")
        || target
            .map(|f| f.lens == "claims_compliance" || f.label.starts_with("legal"))
            .unwrap_or(false);
    if !is_legal_target {
        return true;
    }
    is_regulation_citation(&mv.detail) || is_regulation_citation(&mv.new_evidence)
}
