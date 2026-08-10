mod ask;
mod checks;
mod describe;
mod discourse;
mod fixcheck;
mod humanvoice;
mod improve;
mod input;
mod lens;
mod llm;
mod policy;
mod promptctx;
mod quantify;
mod report;
mod requirements;
mod spec;
mod state;

use anyhow::{anyhow, Context, Result};
use clap::{Parser, Subcommand};
use lens::Finding;
use llm::Llm;
use spec::{Lens, Spec};
use std::path::PathBuf;

#[derive(clap::ValueEnum, Clone, Debug, PartialEq)]
enum Backend {
    /// claude -p subprocess
    ClaudeCli,
    /// OpenRouter REST API (requires OPENROUTER_API_KEY)
    Openrouter,
}

#[derive(Parser, Debug)]
#[command(
    name = "marketing-loop",
    version,
    about = "Multi-perspective (multi-persona) marketing content review pipeline — independent per-persona review followed by discourse cross-validation"
)]
struct Cli {
    #[arg(long, default_value = "claude", global = true)]
    claude_bin: String,
    #[arg(long, value_enum, default_value = "claude-cli", global = true)]
    backend: Backend,
    #[arg(long, global = true)]
    model: Option<String>,
    /// Low-cost model used for simple judgment stages like lens selection, requirements verification, fix check.
    /// Defaults to --model if not specified.
    #[arg(long, global = true)]
    cheap_model: Option<String>,
    #[arg(long, default_value_t = 2, global = true)]
    retries: u32,
    #[arg(long, global = true)]
    verbose: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Independent per-persona review + discourse cross-validation (default pipeline)
    Review {
        #[arg(long)]
        spec: PathBuf,
        #[arg(long)]
        content: PathBuf,
        #[arg(long)]
        content_type: String,
        #[arg(long)]
        requirements: Option<PathBuf>,
        #[arg(long)]
        conventions: Option<PathBuf>,
        #[arg(long)]
        deterministic_results: Option<PathBuf>,
        /// Manually specify lenses (comma-separated). If omitted, the LLM selects based on content-type nature.
        #[arg(long)]
        lenses: Option<String>,
        #[arg(long, default_value = "runs")]
        out: PathBuf,
        #[arg(long, default_value_t = 1)]
        concurrency: usize,
        /// Max number of discourse rounds
        #[arg(long, default_value_t = 2)]
        max_rounds: usize,
        /// Previous round's --out directory (state.json).
        #[arg(long)]
        prior: Option<PathBuf>,
        /// Rewrite confirmed findings/good things in a human review-comment tone and attach to the report
        #[arg(long)]
        human_voice: bool,
    },
    /// Content title, core message, target audience, labels, whether splittable, + TODO scan
    Describe {
        #[arg(long)]
        spec: PathBuf,
        #[arg(long)]
        content: PathBuf,
        #[arg(long)]
        content_type: String,
        #[arg(long)]
        requirements: Option<PathBuf>,
        #[arg(long)]
        conventions: Option<PathBuf>,
        #[arg(long, default_value = "runs")]
        out: PathBuf,
    },
    /// Concrete copy improvement suggestions (before/after)
    Improve {
        #[arg(long)]
        spec: PathBuf,
        #[arg(long)]
        content: PathBuf,
        #[arg(long)]
        content_type: String,
        #[arg(long)]
        requirements: Option<PathBuf>,
        #[arg(long)]
        conventions: Option<PathBuf>,
        #[arg(long, default_value = "runs")]
        out: PathBuf,
    },
    /// Free-form question about the content
    Ask {
        #[arg(long)]
        spec: PathBuf,
        #[arg(long)]
        content: PathBuf,
        #[arg(long)]
        content_type: String,
        #[arg(long)]
        requirements: Option<PathBuf>,
        #[arg(long)]
        conventions: Option<PathBuf>,
        #[arg(long, default_value = "runs")]
        out: PathBuf,
        question: String,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let (llm, cheap_llm) = build_llm(&cli)?;

    match &cli.command {
        Commands::Review {
            spec,
            content,
            content_type,
            requirements,
            conventions,
            deterministic_results,
            lenses,
            out,
            concurrency,
            max_rounds,
            prior,
            human_voice,
        } => run_review(
            &llm,
            &cheap_llm,
            spec,
            content,
            content_type,
            requirements,
            conventions,
            deterministic_results,
            lenses,
            out,
            *concurrency,
            *max_rounds,
            prior,
            *human_voice,
        ),
        Commands::Describe {
            spec,
            content,
            content_type,
            requirements,
            conventions,
            out,
        } => run_describe(
            &llm,
            spec,
            content,
            content_type,
            requirements,
            conventions,
            out,
        ),
        Commands::Improve {
            spec,
            content,
            content_type,
            requirements,
            conventions,
            out,
        } => run_improve(
            &llm,
            spec,
            content,
            content_type,
            requirements,
            conventions,
            out,
        ),
        Commands::Ask {
            spec,
            content,
            content_type,
            requirements,
            conventions,
            out,
            question,
        } => run_ask(
            &llm,
            spec,
            content,
            content_type,
            requirements,
            conventions,
            out,
            question,
        ),
    }
}

/// A (main model, low-cost model) pair. If `--cheap-model` isn't given, the low-cost model is the
/// same as the main model, preserving prior behavior. Both share a single usage tracker to produce a combined total.
fn build_llm(cli: &Cli) -> Result<(Llm, Llm)> {
    let usage = Llm::new_usage_tracker();
    let cheap_model = cli.cheap_model.clone().or_else(|| cli.model.clone());
    let (main_llm, cheap_llm) = match cli.backend {
        Backend::ClaudeCli => (
            Llm::claude_cli(
                cli.claude_bin.clone(),
                cli.model.clone(),
                cli.retries,
                cli.verbose,
                usage.clone(),
            ),
            Llm::claude_cli(
                cli.claude_bin.clone(),
                cheap_model,
                cli.retries,
                cli.verbose,
                usage.clone(),
            ),
        ),
        Backend::Openrouter => (
            Llm::openrouter(cli.model.clone(), cli.retries, cli.verbose, usage.clone())?,
            Llm::openrouter(cheap_model, cli.retries, cli.verbose, usage.clone())?,
        ),
    };
    Ok((main_llm, cheap_llm))
}

#[allow(clippy::too_many_arguments)]
fn run_review(
    llm: &Llm,
    cheap_llm: &Llm,
    spec_path: &PathBuf,
    content_path: &PathBuf,
    content_type: &str,
    requirements_path: &Option<PathBuf>,
    conventions_path: &Option<PathBuf>,
    deterministic_results_path: &Option<PathBuf>,
    lenses_arg: &Option<String>,
    out: &PathBuf,
    concurrency: usize,
    max_rounds: usize,
    prior: &Option<PathBuf>,
    human_voice: bool,
) -> Result<()> {
    let sp = Spec::load(spec_path)?;
    let mut inp = input::normalize(
        content_path,
        content_type,
        requirements_path.as_deref(),
        conventions_path.as_deref(),
        deterministic_results_path.as_deref(),
    )?;
    let out_dir = prepare_out(out)?;

    // Lens selection: if --lenses is manually given, parse it comma-separated; otherwise let the
    // LLM select, then force-add the always lenses.
    let mut selected: Vec<Lens> = match lenses_arg {
        Some(s) => {
            let ids: Vec<String> = s
                .split(',')
                .map(|x| x.trim().to_string())
                .filter(|x| !x.is_empty())
                .collect();
            let mut selected = Vec::with_capacity(ids.len());
            for id in &ids {
                let lens = sp
                    .lens_by_id(id)
                    .ok_or_else(|| anyhow!("Lens id not in spec: {id}"))?;
                selected.push(lens.clone());
            }
            selected
        }
        None => lens::select_lenses(cheap_llm, &sp, content_type)?,
    };
    for l in sp.always_lenses() {
        if !selected.iter().any(|x| x.id == l.id) {
            selected.push(l.clone());
        }
    }
    println!(
        "Selected lenses: {}",
        selected
            .iter()
            .map(|l| l.id.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    );

    // Independent per-lens review (review_all internally runs thread chunks up to concurrency)
    let mut findings: Vec<Finding> = lens::review_all(llm, &sp, &selected, &inp, concurrency)?;
    println!("Lens review complete — {} finding(s)", findings.len());

    // Policy lens (local deterministic)
    let policies = policy::check_all(&sp, &inp);

    // Deterministic checks: if a --deterministic-results file is already injected, prefer that;
    // otherwise fill it via local computation (checks::run_local_checks).
    if inp.deterministic_results.is_none() {
        inp.deterministic_results = Some(checks::run_local_checks(&inp, &sp));
    }

    // discourse cross-validation
    println!("Starting discourse (max {} rounds)", max_rounds);
    let (audit, mut resolved) = discourse::run(llm, &sp, &findings, max_rounds)?;

    // Effort/time estimate (independent of the prior-round merge below).
    let effort = quantify::effort(&inp, selected.len());
    let (best, avg, worst) = quantify::time_estimate(effort);

    // Vs. previous round (--prior): determine whether findings confirmed there were fixed in this
    // content. If STILL_OPEN, fold it back into this round's working set.
    let (round, fix_results): (usize, Option<Vec<fixcheck::FixResult>>) = match prior {
        None => (0, None),
        Some(p) => {
            let ps = state::load(p)?;
            let prior_confirmed: Vec<&Finding> = prior_findings_to_recheck(&ps, &sp);
            let fr = fixcheck::run(cheap_llm, &sp, &inp.content, &prior_confirmed)?;
            for item in &fr {
                if item.status == "STILL_OPEN" {
                    if let Some(orig) = prior_confirmed.iter().find(|f| f.id == item.finding_id) {
                        // Finding ids are generated per-round from scratch (lens id + position
                        // within *this round's* own list — see lens::review_lens), so a carried-over
                        // prior finding can collide with a fresh finding the same lens produces this
                        // round. Namespace it so it can never collide with a same-round-generated id,
                        // which is always exactly "{lens.id}-{n}" with no "prior-" prefix.
                        let mut carried = (*orig).clone();
                        carried.id = prior_finding_id(&carried.id);
                        let carried_id = carried.id.clone();
                        findings.push(carried);
                        resolved.insert(
                            carried_id,
                            discourse::Resolution {
                                status: "CONFIRMED".to_string(),
                                evidence: format!(
                                    "Unresolved from previous round (re-confirmed): {}",
                                    item.evidence
                                ),
                            },
                        );
                    }
                }
            }
            (ps.round + 1, Some(fr))
        }
    };

    // Human review-comment tone rewrite (optional) — based on the confirmed list after the prior merge.
    // Assumption: the current scaffold has no separate lens/function that generates good things
    // (not included in lens.rs), so good_things is always passed as an empty list.
    // Includes CONFIRMED findings and blocking-tier MERGED findings (#17) — see quantify::counts_toward_score.
    let confirmed_after_merge: Vec<&Finding> = findings
        .iter()
        .filter(|f| quantify::counts_toward_score(f, &resolved, &sp))
        .collect();

    // Requirements verification — computed after the prior-round merge above, from the same
    // post-merge confirmed list, so a prior-round finding that's still open (re-confirmed above)
    // is visible to the requirements LLM call as evidence too, not just to score/verdict/report.
    let req_results = requirements::verify(cheap_llm, &sp, &inp, &confirmed_after_merge)?;

    // Quantitative summary + verdict — computed after the prior-round merge above so the console
    // output matches report.md (report::write_review below recomputes these from the same
    // post-merge findings/resolved/confirmed_after_merge).
    let score = quantify::score(&findings, &resolved, &sp);
    let verdict = quantify::verdict(&confirmed_after_merge, &policies, &req_results);

    let good_things: Vec<humanvoice::GoodThing> = Vec::new();
    let hv = if human_voice {
        Some(humanvoice::rewrite(
            llm,
            &sp,
            &inp,
            &confirmed_after_merge,
            &good_things,
        )?)
    } else {
        None
    };

    let deterministic = inp
        .deterministic_results
        .clone()
        .unwrap_or_else(|| serde_json::json!({}));
    report::write_review(
        &out_dir,
        &sp,
        &inp,
        round,
        &findings,
        &resolved,
        &policies,
        &req_results,
        &audit,
        &deterministic,
        fix_results.as_deref(),
        hv.as_deref(),
        &selected,
    )?;

    state::write(
        &out_dir,
        &state::State {
            round,
            findings: findings.clone(),
            resolved: resolved.clone(),
        },
    )?;

    println!("\nDone — verdict={verdict} score={score}/100 effort={effort} estimated time (min) best={best} avg={avg} worst={worst}");
    println!("Report: {}", out_dir.join("report.md").display());
    println!("Next round: --prior {}", out_dir.display());
    println!("{}", llm.usage().summary());
    Ok(())
}

fn run_describe(
    llm: &Llm,
    spec_path: &PathBuf,
    content_path: &PathBuf,
    content_type: &str,
    requirements_path: &Option<PathBuf>,
    conventions_path: &Option<PathBuf>,
    out: &PathBuf,
) -> Result<()> {
    let sp = Spec::load(spec_path)?;
    let inp = input::normalize(
        content_path,
        content_type,
        requirements_path.as_deref(),
        conventions_path.as_deref(),
        None,
    )?;
    let out_dir = prepare_out(out)?;
    let d = describe::run(llm, &sp, &inp)?;
    let todos = describe::todo_sections(&inp.content);
    report::write_describe(&out_dir, &d, &todos)?;
    println!(
        "describe complete: {}",
        out_dir.join("describe.md").display()
    );
    println!("{}", llm.usage().summary());
    Ok(())
}

// Assumption: since the ground truth doesn't specify whether improve goes through the review
// stage, we chose the simpler implementation — skip review (lens selection/review, discourse) and
// call improve::run directly with just the content + spec.
fn run_improve(
    llm: &Llm,
    spec_path: &PathBuf,
    content_path: &PathBuf,
    content_type: &str,
    requirements_path: &Option<PathBuf>,
    conventions_path: &Option<PathBuf>,
    out: &PathBuf,
) -> Result<()> {
    let sp = Spec::load(spec_path)?;
    let inp = input::normalize(
        content_path,
        content_type,
        requirements_path.as_deref(),
        conventions_path.as_deref(),
        None,
    )?;
    let out_dir = prepare_out(out)?;
    let suggestions = improve::run(llm, &sp, &inp)?;
    report::write_improve(&out_dir, &suggestions)?;
    println!(
        "improve complete: {} suggestion(s) — {}",
        suggestions.len(),
        out_dir.join("improve.md").display()
    );
    println!("{}", llm.usage().summary());
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn run_ask(
    llm: &Llm,
    spec_path: &PathBuf,
    content_path: &PathBuf,
    content_type: &str,
    requirements_path: &Option<PathBuf>,
    conventions_path: &Option<PathBuf>,
    out: &PathBuf,
    question: &str,
) -> Result<()> {
    let sp = Spec::load(spec_path)?;
    let inp = input::normalize(
        content_path,
        content_type,
        requirements_path.as_deref(),
        conventions_path.as_deref(),
        None,
    )?;
    let out_dir = prepare_out(out)?;
    let answer = ask::run(llm, &sp, &inp, question)?;
    report::write_ask(&out_dir, question, &answer)?;
    println!(
        "{}\n\nRecorded: {}",
        answer,
        out_dir.join("ask.md").display()
    );
    println!("{}", llm.usage().summary());
    Ok(())
}

fn prepare_out(p: &PathBuf) -> Result<PathBuf> {
    std::fs::create_dir_all(p)
        .with_context(|| format!("Failed to create output directory: {}", p.display()))?;
    Ok(p.clone())
}

/// Findings from a `--prior` round that fix-check should re-verify: whatever counted toward that
/// round's own score/verdict (`quantify::counts_toward_score`, #17) — not just a literal
/// `CONFIRMED` status, so a blocking-tier finding that scored via `MERGED` there isn't silently
/// dropped from cross-round tracking. See issue #27.
fn prior_findings_to_recheck<'a>(state: &'a state::State, spec: &Spec) -> Vec<&'a Finding> {
    state
        .findings
        .iter()
        .filter(|f| quantify::counts_toward_score(f, &state.resolved, spec))
        .collect()
}

/// Namespaces a carried-over `--prior` finding id so it can never collide with a same-round
/// generated id (always exactly "{lens.id}-{n}", never "prior-"-prefixed). Idempotent — a
/// finding that's already namespaced (e.g. carried forward across multiple rounds) isn't
/// double-prefixed. See issue #6: without this, a round's own finding id could collide with a
/// re-confirmed --prior finding's id, silently resurrecting a REJECTED finding as CONFIRMED.
fn prior_finding_id(id: &str) -> String {
    if id.starts_with("prior-") {
        id.to_string()
    } else {
        format!("prior-{id}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::discourse::Resolution;
    use crate::spec::Lens;
    use std::collections::HashMap;

    /// Regression test for #6.
    #[test]
    fn prior_finding_id_adds_prefix_once() {
        assert_eq!(prior_finding_id("copy_craft-1"), "prior-copy_craft-1");
        assert_eq!(prior_finding_id("prior-copy_craft-1"), "prior-copy_craft-1");
    }

    fn test_lens(id: &str, tier: &str) -> Lens {
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

    fn test_finding(id: &str, lens: &str) -> Finding {
        Finding {
            id: id.to_string(),
            lens: lens.to_string(),
            persona: "persona".to_string(),
            severity: "P1".to_string(),
            label: lens.to_string(),
            block_ref: "b:0".to_string(),
            claim: "claim".to_string(),
            evidence: "evidence".to_string(),
            impact: String::new(),
            recommendation: String::new(),
        }
    }

    /// Regression test for #27: a blocking-tier finding that scored via MERGED in the prior
    /// round (per #17) must still be picked up for --prior fix-check re-verification, not just
    /// findings with a literal CONFIRMED status.
    #[test]
    fn prior_findings_to_recheck_includes_blocking_tier_merged() {
        let spec = Spec {
            name: "t".into(),
            context: String::new(),
            lenses: vec![
                test_lens("claims_compliance", "blocking"),
                test_lens("copy_craft", "standard"),
            ],
            deterministic_checks: vec![],
            labels: vec!["l".into()],
            content_length_limit: 0,
            disclaimer_required_types: vec![],
            required_brand_terms: vec![],
        };
        let mut resolved: HashMap<String, Resolution> = HashMap::new();
        resolved.insert(
            "claims_compliance-1".to_string(),
            Resolution {
                status: "MERGED".to_string(),
                evidence: String::new(),
            },
        );
        resolved.insert(
            "copy_craft-1".to_string(),
            Resolution {
                status: "MERGED".to_string(),
                evidence: String::new(),
            },
        );
        let state = state::State {
            round: 0,
            findings: vec![
                test_finding("claims_compliance-1", "claims_compliance"),
                test_finding("copy_craft-1", "copy_craft"),
            ],
            resolved,
        };

        let recheck = prior_findings_to_recheck(&state, &spec);
        let ids: Vec<&str> = recheck.iter().map(|f| f.id.as_str()).collect();
        assert!(ids.contains(&"claims_compliance-1"));
        assert!(!ids.contains(&"copy_craft-1"));
    }
}
