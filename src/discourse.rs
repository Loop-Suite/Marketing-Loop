use crate::lens::Finding;
use crate::llm::Llm;
use crate::spec::Spec;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub const DISCOURSE_SYSTEM: &str =
    "You are a marketing review panel that cross-validates findings from multiple personas. \
Do not agree or rebut without substance. Use AGREE only when citing new evidence (new_evidence). \
This round must include at least one CHALLENGE. \
Respond strictly in the specified JSON schema only.";

/// Ported as-is from the original discourse.rs's AGREE/CHALLENGE/CONNECT/SURFACE structure.
/// Reviewer identity (persona) is never exposed — judged solely by target_finding_id/evidence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Move {
    #[serde(
        rename = "move",
        default,
        deserialize_with = "crate::llm::null_as_default"
    )]
    pub kind: String, // AGREE|CHALLENGE|CONNECT|SURFACE
    // SURFACE describes a newly-discovered issue by definition, so it has no existing finding to
    // point at — the model legitimately omits this field for that move kind (or sends `null`).
    #[serde(default, deserialize_with = "crate::llm::null_as_default")]
    pub target_finding_id: String,
    #[serde(default, deserialize_with = "crate::llm::null_as_default")]
    pub new_evidence: String,
    #[serde(default, deserialize_with = "crate::llm::null_as_default")]
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
         - CONNECT: relates two or more findings. List ALL of their ids in target_finding_id, comma-separated (e.g. \"id1,id2\"), and describe the cause/effect relationship in detail.\n\
         - SURFACE: describe a newly discovered issue in detail along with evidence. It has no existing target, so leave target_finding_id as an empty string \"\".\n\
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
    #[serde(default, deserialize_with = "crate::llm::null_as_default")]
    moves: Vec<Move>,
}

fn run_round_call(
    llm: &Llm,
    spec: &Spec,
    findings: &[Finding],
    round: usize,
) -> Result<DiscourseRound> {
    let prompt = build_round_prompt(spec, findings, round);
    let v = llm
        .json(&prompt, Some(DISCOURSE_SYSTEM))
        .with_context(|| format!("discourse round {round} failed"))?;
    let mr: MovesResponse = serde_json::from_value(v)
        .with_context(|| format!("discourse round {round} JSON schema mismatch"))?;
    Ok(DiscourseRound {
        round,
        moves: mr.moves,
    })
}

/// CONNECT relates two or more findings at once (per the prompt's instruction), so unlike
/// AGREE/CHALLENGE (always exactly one target), target_finding_id for a CONNECT move is a
/// comma-separated list of ids, e.g. "seo-1,copy_craft-2". Splits and trims it into individual ids.
fn connect_target_ids(target_finding_id: &str) -> Vec<&str> {
    target_finding_id
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect()
}

/// Computes the final Resolution per finding_id, factoring in only moves that weren't invalidated.
/// Priority: valid CHALLENGE (unrebutted) > CONNECT > only valid AGREE present > default UNCERTAIN.
fn resolve_findings(
    findings: &[Finding],
    rounds: &[DiscourseRound],
) -> HashMap<String, Resolution> {
    let all_moves: Vec<&Move> = rounds.iter().flat_map(|r| r.moves.iter()).collect();

    findings
        .iter()
        .map(|f| {
            let valid_challenges: Vec<&&Move> = all_moves
                .iter()
                .filter(|m| {
                    m.kind == "CHALLENGE"
                        && m.target_finding_id == f.id
                        && validate_challenge_on_legal(m, findings)
                })
                .collect();
            let connects: Vec<&&Move> = all_moves
                .iter()
                .filter(|m| {
                    m.kind == "CONNECT"
                        && connect_target_ids(&m.target_finding_id).contains(&f.id.as_str())
                })
                .collect();
            let valid_agrees: Vec<&&Move> = all_moves
                .iter()
                .filter(|m| m.kind == "AGREE" && m.target_finding_id == f.id && validate_agree(m))
                .collect();

            let resolution = if !valid_challenges.is_empty() {
                let ev = valid_challenges
                    .iter()
                    .map(|m| m.detail.as_str())
                    .collect::<Vec<_>>()
                    .join(" / ");
                Resolution {
                    status: "REJECTED".to_string(),
                    evidence: ev,
                }
            } else if !connects.is_empty() {
                // CONNECT does not replace the original finding — it stays in the findings list, only status becomes MERGED.
                let ev = connects
                    .iter()
                    .map(|m| m.detail.as_str())
                    .collect::<Vec<_>>()
                    .join(" / ");
                Resolution {
                    status: "MERGED".to_string(),
                    evidence: ev,
                }
            } else if !valid_agrees.is_empty() {
                let ev = valid_agrees
                    .iter()
                    .map(|m| m.new_evidence.as_str())
                    .collect::<Vec<_>>()
                    .join(" / ");
                Resolution {
                    status: "CONFIRMED".to_string(),
                    evidence: ev,
                }
            } else {
                Resolution {
                    status: "UNCERTAIN".to_string(),
                    evidence: "No basis for a verdict found in the cross-validation round"
                        .to_string(),
                }
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
            dr = run_round_call(llm, spec, findings, round)
                .context("re-request for missing CHALLENGE failed")?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lens::Finding;

    fn finding(id: &str, lens: &str) -> Finding {
        Finding {
            id: id.to_string(),
            lens: lens.to_string(),
            persona: "p".to_string(),
            severity: "P1".to_string(),
            label: "label".to_string(),
            block_ref: "b:0".to_string(),
            claim: "c".to_string(),
            evidence: "e".to_string(),
            impact: String::new(),
            recommendation: String::new(),
        }
    }

    fn mv(kind: &str, target: &str, detail: &str, new_evidence: &str) -> Move {
        Move {
            kind: kind.to_string(),
            target_finding_id: target.to_string(),
            new_evidence: new_evidence.to_string(),
            detail: detail.to_string(),
        }
    }

    #[test]
    fn connect_target_ids_splits_and_trims_comma_list() {
        assert_eq!(
            connect_target_ids("a-1, b-2 ,c-3"),
            vec!["a-1", "b-2", "c-3"]
        );
    }

    #[test]
    fn connect_target_ids_empty_string_yields_no_ids() {
        assert!(connect_target_ids("").is_empty());
    }

    /// Regression test for #10: a CONNECT move's target_finding_id can hold multiple
    /// comma-joined ids (real observed model output, e.g. "claims_compliance-3,copy_craft-1") —
    /// every referenced finding must resolve to MERGED, not just the first.
    #[test]
    fn connect_move_merges_all_comma_joined_targets() {
        let findings = vec![finding("a-1", "lens_a"), finding("b-1", "lens_b")];
        let round = DiscourseRound {
            round: 1,
            moves: vec![mv("CONNECT", "a-1,b-1", "related", "")],
        };
        let resolved = resolve_findings(&findings, &[round]);
        assert_eq!(resolved["a-1"].status, "MERGED");
        assert_eq!(resolved["b-1"].status, "MERGED");
    }

    #[test]
    fn agree_without_new_evidence_is_invalid() {
        assert!(!validate_agree(&mv("AGREE", "a-1", "", "")));
        assert!(validate_agree(&mv(
            "AGREE",
            "a-1",
            "",
            "concrete new evidence"
        )));
    }

    #[test]
    fn agree_with_new_evidence_confirms_the_finding() {
        let findings = vec![finding("a-1", "lens_a")];
        let round = DiscourseRound {
            round: 1,
            moves: vec![mv("AGREE", "a-1", "", "independently verified")],
        };
        let resolved = resolve_findings(&findings, &[round]);
        assert_eq!(resolved["a-1"].status, "CONFIRMED");
    }

    #[test]
    fn challenge_on_legal_finding_requires_regulation_citation() {
        let findings = vec![finding("claims_compliance-1", "claims_compliance")];
        let stylistic = mv(
            "CHALLENGE",
            "claims_compliance-1",
            "I just don't like the tone",
            "",
        );
        assert!(!validate_challenge_on_legal(&stylistic, &findings));
        let cited = mv(
            "CHALLENGE",
            "claims_compliance-1",
            "제17조 2항에 따라 문제 없음",
            "",
        );
        assert!(validate_challenge_on_legal(&cited, &findings));
    }

    #[test]
    fn challenge_on_non_legal_finding_is_always_valid() {
        let findings = vec![finding("copy_craft-1", "copy_craft")];
        let stylistic = mv("CHALLENGE", "copy_craft-1", "just a preference", "");
        assert!(validate_challenge_on_legal(&stylistic, &findings));
    }

    #[test]
    fn is_regulation_citation_detects_article_and_clause_markers() {
        assert!(is_regulation_citation("제17조에 따라"));
        assert!(is_regulation_citation("2항 위반"));
        assert!(is_regulation_citation("관련 규정 위반"));
        assert!(is_regulation_citation("§230"));
        assert!(!is_regulation_citation("그냥 취향 문제"));
    }

    #[test]
    fn findings_with_no_moves_are_uncertain() {
        let findings = vec![finding("a-1", "lens_a")];
        let resolved = resolve_findings(&findings, &[]);
        assert_eq!(resolved["a-1"].status, "UNCERTAIN");
    }

    /// Regression test for #18: an explicit JSON null on the required "move" field must not
    /// crash deserialization — the same failure family #9 fixed for other fields.
    #[test]
    fn move_tolerates_explicit_null_kind() {
        let v = serde_json::json!({
            "move": null, "target_finding_id": "a-1", "new_evidence": null, "detail": null
        });
        let m: Move =
            serde_json::from_value(v).expect("explicit null must not crash deserialization");
        assert_eq!(m.kind, "");
    }

    /// Regression test for #9: the LLM-response envelope must tolerate an explicit top-level
    /// `"moves": null` without crashing.
    #[test]
    fn moves_response_tolerates_null_moves_array() {
        let v = serde_json::json!({"moves": null});
        let mr: MovesResponse =
            serde_json::from_value(v).expect("explicit null must not crash deserialization");
        assert!(mr.moves.is_empty());
    }
}
