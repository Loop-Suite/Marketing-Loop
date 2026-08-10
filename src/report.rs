use crate::describe;
use crate::discourse;
use crate::fixcheck;
use crate::improve;
use crate::input::Input;
use crate::lens::Finding;
use crate::policy;
use crate::quantify;
use crate::requirements;
use crate::spec::Lens;
use crate::spec::Spec;
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::Path;

fn severity_rank(s: &str) -> u8 {
    match s {
        "P0" => 0,
        "P1" => 1,
        "P2" => 2,
        "P3" => 3,
        _ => 4,
    }
}

/// Renders spec.deterministic_checks as a table, matched against the deterministic
/// (check_id -> {status, evidence}) results. Items with no result show NOT_RUN (counterpart to the original report.rs).
fn deterministic_table(spec: &Spec, deterministic: &serde_json::Value) -> String {
    let mut md = String::new();
    md.push_str("| Check | Expected tool | Status | Evidence |\n|---|---|---|---|\n");
    for c in &spec.deterministic_checks {
        let entry = deterministic.get(&c.id);
        let status = entry
            .and_then(|e| e.get("status"))
            .and_then(|s| s.as_str())
            .unwrap_or("NOT_RUN")
            .to_string();
        let evidence = entry
            .and_then(|e| e.get("evidence"))
            .and_then(|s| s.as_str())
            .unwrap_or("")
            .to_string();
        md.push_str(&format!(
            "| {} | {} | {} | {} |\n",
            c.title, c.tool, status, evidence
        ));
    }
    md
}

/// Renders the review subcommand's result as report.md, following the field order in
/// design-spec.md §6 (verdict → policy checks → quantitative summary → requirements → findings →
/// good things → deterministic checks → discourse audit → human-voice → vs. previous round).
#[allow(clippy::too_many_arguments)]
pub fn write_review(
    out_dir: &Path,
    spec: &Spec,
    input: &Input,
    round: usize,
    findings: &[crate::lens::Finding],
    resolved: &HashMap<String, discourse::Resolution>,
    policies: &[policy::PolicyResult],
    requirements: &Option<Vec<requirements::RequirementCheck>>,
    discourse_rounds: &[discourse::DiscourseRound],
    deterministic: &serde_json::Value,
    fix_results: Option<&[fixcheck::FixResult]>,
    human_voice: Option<&str>,
    selected_lenses: &[Lens],
) -> Result<()> {
    let mut md = String::new();

    // Only CONFIRMED findings are adopted (shared by the Findings table and verdict/score computation).
    let mut confirmed: Vec<&Finding> = findings
        .iter()
        .filter(|f| resolved.get(&f.id).map(|r| r.status.as_str()) == Some("CONFIRMED"))
        .collect();
    confirmed.sort_by_key(|f| severity_rank(&f.severity));

    let verdict = quantify::verdict(&confirmed, policies, requirements);
    let score = quantify::score(findings, resolved);
    let effort = quantify::effort(input, selected_lenses.len());
    let (time_best, time_average, time_worst) = quantify::time_estimate(effort);

    md.push_str(&format!(
        "# Marketing Content Review — {} (round {})\n\n",
        spec.name, round
    ));
    md.push_str(&format!(
        "**Verdict: {}**  ·  Score: {}/100  ·  Effort: {}/5  ·  Content type: {}\n\n",
        verdict, score, effort, input.content_type
    ));
    md.push_str(&format!(
        "Selected lenses: {}\n\n",
        selected_lenses
            .iter()
            .map(|l| l.title.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    ));

    if let Some(frs) = fix_results {
        if !frs.is_empty() {
            md.push_str(
                "## Vs. previous round\n\n| Finding | Status | Evidence |\n|---|---|---|\n",
            );
            for f in frs {
                md.push_str(&format!(
                    "| {} | {} | {} |\n",
                    f.finding_id, f.status, f.evidence
                ));
            }
            md.push('\n');
        }
    }

    md.push_str("## Policy checks\n\n| Policy | Status | Evidence |\n|---|---|---|\n");
    for p in policies {
        md.push_str(&format!(
            "| {} | {} | {} |\n",
            p.check,
            p.status.label(),
            p.evidence
        ));
    }
    md.push('\n');

    md.push_str("## Quantitative Summary\n\n");
    md.push_str(&format!(
        "- estimated_effort: {}/5\n- review time cost: best {} min / average {} min / worst {} min\n",
        effort, time_best, time_average, time_worst
    ));
    if confirmed.is_empty() {
        md.push_str("- No deductions (no CONFIRMED findings)\n\n");
    } else {
        md.push_str("- Deduction breakdown:\n");
        for f in &confirmed {
            md.push_str(&format!(
                "  - {} ({}, {}): -{} points\n",
                f.id,
                f.severity,
                f.label,
                quantify::severity_penalty(&f.severity)
            ));
        }
        md.push('\n');
    }

    md.push_str("## Requirements Verification\n\n");
    match requirements {
        None => md.push_str("(Requirements not provided — verification skipped)\n\n"),
        Some(reqs) if reqs.is_empty() => md.push_str("(No requirements)\n\n"),
        Some(reqs) => {
            md.push_str("| Requirement | Status | Evidence or gap |\n|---|---|---|\n");
            for r in reqs {
                md.push_str(&format!(
                    "| {} | {} | {} |\n",
                    r.requirement, r.status, r.evidence
                ));
            }
            md.push('\n');
        }
    }

    md.push_str("## Findings\n\n");
    md.push_str(&format!("Allowed labels: {}\n\n", spec.labels_prompt()));
    md.push_str("| ID | Priority | Label | Reviewer | Block | Evidence | Impact | Recommendation | Discourse result |\n|---|---|---|---|---|---|---|---|---|\n");
    for f in &confirmed {
        let discourse_result = resolved
            .get(&f.id)
            .map(|r| r.evidence.as_str())
            .unwrap_or("");
        md.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} | {} | {} | {} |\n",
            f.id,
            f.severity,
            f.label,
            f.persona,
            f.block_ref,
            f.evidence,
            f.impact,
            f.recommendation,
            discourse_result
        ));
    }
    md.push('\n');

    let rejected: Vec<&Finding> = findings
        .iter()
        .filter(|f| resolved.get(&f.id).map(|r| r.status.as_str()) == Some("REJECTED"))
        .collect();
    if !rejected.is_empty() {
        md.push_str("### Rejected Candidates\n\n");
        for f in &rejected {
            let reason = resolved
                .get(&f.id)
                .map(|r| r.evidence.as_str())
                .unwrap_or("");
            md.push_str(&format!("- {} ({}) — {}\n", f.id, f.block_ref, reason));
        }
        md.push('\n');
    }

    // MERGED findings are excluded from the main Findings table (only CONFIRMED is shown there)
    // but discourse.rs's own comment says a merged finding "stays in the findings list,
    // cross-referenced" — so it needs somewhere to actually be visible, same as Rejected/Uncertain.
    let merged: Vec<&Finding> = findings
        .iter()
        .filter(|f| resolved.get(&f.id).map(|r| r.status.as_str()) == Some("MERGED"))
        .collect();
    if !merged.is_empty() {
        md.push_str("### Merged Findings\n\n");
        for f in &merged {
            let cross_ref = resolved
                .get(&f.id)
                .map(|r| r.evidence.as_str())
                .unwrap_or("");
            md.push_str(&format!(
                "- {} ({}) — {} — {}\n",
                f.id, f.block_ref, f.claim, cross_ref
            ));
        }
        md.push('\n');
    }

    let uncertain: Vec<&Finding> = findings
        .iter()
        .filter(|f| resolved.get(&f.id).map(|r| r.status.as_str()) == Some("UNCERTAIN"))
        .collect();
    if !uncertain.is_empty() {
        md.push_str("### Needs Verification\n\n");
        for f in &uncertain {
            let note = resolved
                .get(&f.id)
                .map(|r| r.evidence.as_str())
                .unwrap_or("");
            md.push_str(&format!("- {} ({}) — {}\n", f.id, f.block_ref, note));
        }
        md.push('\n');
    }

    // NOTE (assumption): the write_review signature doesn't receive good_things data (unlike the
    // original report.rs, this file doesn't take a good_things parameter), so it always shows
    // "not observed". This is a workaround within this file per the no-signature-change rule —
    // to actually surface good things, the upstream pipeline needs to add a good_things argument to write_review.
    md.push_str("## Good Things\n\nnot observed\n\n");

    md.push_str("## Deterministic checks\n\n");
    md.push_str(&deterministic_table(spec, deterministic));
    md.push('\n');

    md.push_str("## Discourse audit\n\n");
    md.push_str("| Round | Move | Target | Detail | New evidence |\n|---|---|---|---|---|\n");
    for dr in discourse_rounds {
        for mv in &dr.moves {
            // Invalidation rule (design-spec.md §4): AGREE requires new evidence; a CHALLENGE
            // targeting a claims_compliance finding is accepted only if evidence-based. Other moves are always valid.
            let valid = match mv.kind.as_str() {
                "AGREE" => discourse::validate_agree(mv),
                "CHALLENGE" => {
                    let target_lens = findings
                        .iter()
                        .find(|f| f.id == mv.target_finding_id)
                        .map(|f| f.lens.as_str());
                    if target_lens == Some("claims_compliance") {
                        discourse::validate_challenge_on_legal(mv, findings)
                    } else {
                        true
                    }
                }
                _ => true,
            };
            let move_label = if valid {
                mv.kind.clone()
            } else {
                format!("{} (invalid)", mv.kind)
            };
            md.push_str(&format!(
                "| {} | {} | {} | {} | {} |\n",
                dr.round, move_label, mv.target_finding_id, mv.detail, mv.new_evidence
            ));
        }
    }

    if let Some(hv) = human_voice {
        md.push_str("\n## Human-voice Review\n\n");
        md.push_str(hv);
        md.push('\n');
    }

    let path = out_dir.join("report.md");
    std::fs::write(&path, md).with_context(|| format!("Failed to write {}", path.display()))?;
    Ok(())
}

pub fn write_describe(out_dir: &Path, d: &describe::Describe, todos: &[String]) -> Result<()> {
    let mut md = String::new();
    md.push_str(&format!("# {}\n\n{}\n\n", d.title, d.summary));
    md.push_str("## Walkthrough\n\n");
    md.push_str(&format!("{}\n\n", d.walkthrough));
    md.push_str(&format!("## Labels\n\n{}\n\n", d.labels.join(", ")));
    md.push_str(&format!(
        "## can_be_split\n\n{} — {}\n\n",
        d.can_be_split, d.can_be_split_note
    ));
    md.push_str("## TODO/FIXME (new lines, deterministic scan)\n\n");
    if todos.is_empty() {
        md.push_str("None\n");
    } else {
        for t in todos {
            md.push_str(&format!("- {}\n", t));
        }
    }
    let path = out_dir.join("describe.md");
    std::fs::write(&path, md).with_context(|| format!("Failed to write {}", path.display()))?;
    Ok(())
}

pub fn write_improve(out_dir: &Path, suggestions: &[improve::Suggestion]) -> Result<()> {
    let mut md = String::new();
    md.push_str("# Copy Improvement Suggestions\n\n");
    if suggestions.is_empty() {
        md.push_str("No suggestions\n");
    }
    for s in suggestions {
        md.push_str(&format!(
            "## {} — {} [{}]\n\n",
            s.relevant_block, s.one_sentence_summary, s.label
        ));
        md.push_str(&format!("{}\n\n", s.suggestion_content));
        md.push_str(&format!("```\n// before\n{}\n```\n\n", s.existing_content));
        md.push_str(&format!("```\n// after\n{}\n```\n\n", s.improved_content));
    }
    let path = out_dir.join("improve.md");
    std::fs::write(&path, md).with_context(|| format!("Failed to write {}", path.display()))?;
    Ok(())
}

/// ask.md is append-only — new Q&A is appended after existing content, not prepended.
pub fn write_ask(out_dir: &Path, question: &str, answer: &str) -> Result<()> {
    let path = out_dir.join("ask.md");
    let mut entry = format!("## Q: {}\n\n{}\n\n---\n\n", question, answer);
    if path.exists() {
        let prev = std::fs::read_to_string(&path).unwrap_or_default();
        entry = prev + &entry;
    }
    std::fs::write(&path, entry).with_context(|| format!("Failed to write {}", path.display()))?;
    Ok(())
}
