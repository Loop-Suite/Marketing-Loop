# marketing-loop Research Survey

## 1. Overview

The core structure of Code-Review-Loop (a Rust-based persona-review CLI) is **(1) independent per-persona review → (2) anonymized cross-debate via discourse (forced CHALLENGE) → (3) deterministic verdict derivation**. To judge whether this structure can be ported to the marketing-content-verification domain (marketing-loop), we conducted two rounds of research covering (A) the Claude Code skill ecosystem and GitHub OSS, (B) the commercial/open-source marketing-automation landscape and architecture patterns, and (C) adjacent domains (outside code review) that have a discourse structure (independent verdict → cross-debate → consensus). This document consolidates the three sources into "what already exists, and what is the gap."

## 2. Claude Code skill ecosystem survey results

### List of marketing skill repositories

| Repo | ★ | Characteristics | Discourse stage |
|---|---|---|---|
| ericosiu/ai-marketing-skills | 3.2k | content-ops (expert-panel): 7-10 personas auto-assembled per content type → independent 0-100 scoring → weighted average (deterministic) → if under 90, rewrite of top-3 weaknesses, up to 3 rounds | none |
| coreyhaines31/marketingskills | 42k | Collection of CRO/copy/SEO/analytics skills | unconfirmed |
| zubair-trabzada/ai-marketing-claude | 2.2k | 15 skills + parallel subagent fan-out | unconfirmed |
| OpenClaudia/openclaudia-skills | 601 | 34 skills across SEO/content/email/ads/analytics | unconfirmed |
| msitarzewski/agency-agents | 137k | 230+ persona markdown files such as Growth Hacker/SEO/Social Strategist, single-call style | none |
| thatrebeccarae/claude-marketing | 84 | 3-tier structure of SKILL.md/REFERENCE.md/EXAMPLES.md | unconfirmed |
| alirezarezvani/claude-skills | 23.4k | catalog of 345 skills | unconfirmed |
| boraoztunc/skills | 213 | copywriting/SEO/design | unconfirmed |

Conclusion: SKILL.md-format marketing skills are abundant, but a skill with Code-Review-Loop-style discourse cross-verification is **unconfirmed**.

### GitHub AI marketing-automation open source (multi-agent/persona)

- crewAIInc/crewAI-examples — Content Creator Flow, multi-crew generation of blog/LinkedIn/research reports.
- shreyas-lyzr/marketing-agent (4★), stevielkim/Seo-Agents (1★) — small-scale, insufficiently validated.

Conclusion: most are "generate/audit" pipelines, not review-cross-verification structures.

### Closest structural match: ericosiu/ai-marketing-skills

Structure of `content-ops/SKILL.md`, `content-eval/SKILL.md`:

- `experts/*.md` — persona profiles organized by content type (linkedin, instagram, newsletter, x-articles) plus domain. humanizer (1.5x weight) and brand-voice are always included.
- Auto-assembles 7-10 personas matched to the content type.
- Selects scoring criteria per content type from `scoring-rubrics/*.md`.
- Each expert scores independently on a 0-100 scale → weighted average (deterministic aggregation) → if under 90, derives the top-3 weaknesses and rewrites → repeats up to 3 rounds.
- Outputs a per-round table (Expert | Score | Feedback) plus a final PASS/NEEDS WORK verdict.

**Reusable points**: the `experts/*.md` convention, separating out `scoring-rubrics`, the weighted-average + repeat-threshold deterministic loop, and the per-round table + badge output.

**Limitation**: there is no explicit AGREE/CHALLENGE/CONNECT/SURFACE-style debate stage between personas — this is confirmed, not a guess.

Adversarial cross-examination (for code review — gaurav-yadav/adversarial-ai-review, 4★; 3062-in-zamud/ai-multi-review, 0★) exists, but a marketing port of it is **unconfirmed**.

## 3. Commercial/open-source marketing-automation landscape

### Commercial tools

| Tool | Structure |
|---|---|
| Jasper | Jasper IQ (brand-voice/governance context hub) → 100+ specialized agents. A "brand rules injected before generation" approach (the opposite of marketing-loop's "verify after generation" orientation) |
| HubSpot Breeze | Dual structure of Core Agents (4 fixed, conservative model) vs. Marketplace specialized agents (latest model). Autonomous-execution style, not persona cross-verification |
| Anyword, Persado | LLM generation + a separate predictive scoring layer (statistical model) for quantitative grading. Similar in spirit to separating LLM judgment from deterministic scoring, but it's a single predictive model rather than independent persona review |
| Copy.ai GTM | Mixed Workflows/Actions/Agents composition, no explicit persona-based review |
| Mutiny | Moving toward agent-first; review-stage structure unclear (unconfirmed) |

Overall observation: none of the commercial tools surveyed provide evidence supporting a 3-stage "independent persona lens → discourse cross-verification → deterministic verdict" structure. Most are a 2-stage structure of "generation engine + a single predictive/scoring layer."

### Architecture-pattern references

- Braze "Agentic workflows" — 4 layers: context/agent/orchestration/feedback-loop. Governance section mentions sandbox testing, human-in-the-loop, and audit trails.
- McKinsey — emphasizes "accelerating idea-to-vetting" but does not present a concrete architecture.

### Actual implementations of multi-angle persona cross-verification

- **Locomotive Agency, "Synthetic Personas for Landing Page Optimization"** — multiple personas independently and in parallel evaluate the same content → SSR (semantic similarity) and FLR (explicit rating) dual scoring is combined, with a re-review loop on disagreement. The real-world case closest to a discourse stage (consensus/conflict detection) design, but it serves a different purpose (simulating customer reaction, not a quality gate).
- Multi-Agent Debate for LLM Judges (OpenReview), FairAgent (MDPI) — not the marketing domain, citable only as structural grounding.
- An open-source framework with parallel legal/brand/SEO/conversion 4-lens review: **unconfirmed**.

### Claude-backed marketing-automation cases

- **Anthropic's official "How Anthropic uses Claude in Marketing"** — Google Ads copy: campaign data + keywords in → verified against brand tone/product accuracy/RSA best practices → CSV out. AI generation (draft) and human review (value proposition, tone, differentiation) are clearly separated.
- **Claude Cowork marketing-operations case** — the closest structural match: a dispatcher skill (routing only) → 5 specialist skills (task execution) → an audit skill that performs actual verification independently, with no prior context, in a "fresh verifier" pattern. In principle consistent with Code-Review-Loop's requirement for lens independence.
- Advolve case study — Claude as the central orchestrator; detailed pipeline structure not confirmed.

**Synthesis**: the 3-stage structure of multi-persona independent review + discourse cross-verification + deterministic verdict has no precedent in the marketing-automation market → a differentiation point. Locomotive Agency's SSR/FLR dual-scoring + disagreement-detection logic, and Anthropic's own dispatcher → specialist → audit (fresh verifier) principle, are the most directly useful references.

## 4. Adjacent-domain cases of the discourse structure (independent verdict → cross-debate → consensus)

Baseline (Code-Review-Loop's `discourse.rs`): reviewer identity stripped (leaving only id/file:line/claim/evidence), then anonymous debate. AGREE is valid only when citing new evidence (file:line) not already in the finding. CHALLENGE is forced at least once per round; if absent, one automatic retry.

### Content moderation / AI safety — closest match

- **HAJailBench (arXiv:2511.06396)** — a Critic makes a first-pass risk verdict with grounds → a Defender rebuts and offers alternative interpretations while making its own verdict → rounds repeat until convergence → a Judge finalizes only after debate ends. Verdict-update rules confirmed to be codified: duplicate detection via a Ratcliff-Obershelp similarity threshold of 0.85, early termination when the same risk band is reached. 3 rounds is cost/accuracy-optimal (ablation study included; 4-5 rounds degrade performance).
- **CourtGuard (arXiv:2602.22557)** — only the abstract was accessible, detailed algorithm unconfirmed.

### Fact-checking / journalism

- **PROClaim (arXiv:2603.28488)** — 3 roles (Plaintiff/Defense/Judge), 5 steps per round, up to 10 rounds. Codified early termination: reflection plateau (Δ<0.05 for 2 consecutive rounds), novelty exhaustion (average <0.10), and the critic's `debate_resolved` signal. Final verdict by 2/3 majority among 3 judges. However, there is no independent first-pass stage before debate begins directly — inconsistent with the 3-stage structure required here.
- Debating Truth (arXiv:2507.19090) — repo inaccessible, details unconfirmed.
- FC-MAD, Tool-MAD (arXiv:2601.04742) — only mentions judge-guided consensus, code-level update rules unconfirmed.

### Legal review — the clearest match to the 3-stage structure

- **Investigating Multi-Agent Deliberation in Law (arXiv:2606.30906)** — 3 LLMs independently produce a first-pass prediction → over 2 rounds, each sees the others' responses and revises its own → majority vote. Model-call count is explicit (initial 3 + 2 rounds × 3 = 9 calls). 3-Ply approach: plaintiff/defendant initial argument → rebuttal → re-rebuttal → judge's final decision. The closest match to the required structure, with clarity at the algorithmic level.
- AgentsBench (MDPI) — mentions Round 1 independent analysis → (if consensus fails) Round 2 rebuttal → consensus check → judge arbitration, but the original text returned a 403 and details could not be verified; low confidence.

### Academic peer review

- **AgentReview (arXiv:2406.12708)** — Phase I: 3 reviewers independently give a first-pass score (1-10) in isolation → Phase III: the AC requests an updated review reflecting the rebuttal → the AC makes the final decision via meta-review (fixed 32% accept rate, reflecting the actual average acceptance rate at ICLR 2020-2023). Nowhere in the paper's body (including the Limitations section) is there a description that codifies an AGREE/CHALLENGE-style explicit agreement/rebuttal protocol — though this is not something the authors stated as a limitation themselves, but an observation (inference) from reviewing the methodology and finding no such protocol appears. Whether the discourse update rules are codified is close to **unconfirmed**, since the original text does not describe it.

### Advertising review boards

No cases found — **unconfirmed**, reconfirmed.

**Synthesis**: no case across the domain has codified rules at Code-Review-Loop's level (forced CHALLENGE, anonymization, mandatory new file:line evidence). Closest matches: HAJailBench (quantified termination conditions), legal MAD (even model-call counts specified). Frameworks using only a strong "courtroom debate" metaphor (PROClaim/CourtGuard/AgentsBench) either lack an independent first-pass stage or have undisclosed details.

## 5. Overall conclusion

### Summary of research grounding

- No precedent for the 3-stage "independent persona review → discourse cross-verification → deterministic verdict" structure was confirmed anywhere in the marketing domain itself — not in the skill ecosystem, nor in the commercial/open-source automation landscape.
- Proximity ranking: ericosiu content-ops (independent scoring + weighted-average deterministic loop, no discourse) > Anthropic Cowork's dispatcher → specialist → audit (fresh verifier, consistent with the lens-independence principle) > Locomotive Agency's SSR/FLR (similar to cross-verification but a different purpose, not a quality gate).
- The codified rules of discourse itself (forced CHALLENGE, anonymization, mandatory new file:line evidence) were unconfirmed not just in marketing but across adjacent domains broadly. The two closest cases are HAJailBench (quantified termination conditions) and legal MAD (model-call count and 3-Ply structure explicit).

### Implementation references

- The `experts/*.md` + `scoring-rubrics/*.md` separation convention (ericosiu/ai-marketing-skills)
- Auto-assembling 7-10 personas per content type, with a 1.5x humanizer weight (ericosiu)
- Independent scoring (0-100) → weighted-average deterministic aggregation → rewrite of top-3 weaknesses below a threshold (90), up to 3 rounds (ericosiu)
- Per-round Expert | Score | Feedback table + PASS/NEEDS WORK badge output (ericosiu)
- A 3-stage dispatcher (routing) → specialist (execution) → audit ("fresh verifier," independent verification with no prior context) pipeline (Anthropic Cowork)
- SSR (semantic similarity) + FLR (explicit rating) dual scoring + a re-review loop on disagreement (Locomotive Agency)
- Codified verdict-update rules: duplicate detection via a Ratcliff-Obershelp similarity threshold of 0.85 + early termination at the same risk band + 3 rounds as optimal (ablation) (HAJailBench)
- 3-Ply debate (initial argument → rebuttal → re-rebuttal → final decision) + explicit model-call count (9 = initial 3 + 2 rounds × 3) (legal MAD, arXiv:2606.30906)
- Examples of codified early termination: reflection plateau (Δ<0.05 for 2 consecutive rounds), novelty exhaustion (average <0.10), 3-judge 2/3 majority vote (PROClaim — note, however, that it has no independent first-pass stage)

### Items to apply immediately

- The `experts/*.md`, `scoring-rubrics/*.md` separation convention can be adopted as-is.
- A weighted-average + repeat-threshold (e.g., rework if under 90, up to 3 rounds) deterministic loop is compatible with Code-Review-Loop's verdict logic.
- The dispatcher → specialist → audit (fresh verifier) principle — portable as an "independent context" requirement for the marketing-content audit stage.
- HAJailBench-style similarity-threshold-based duplicate-evidence detection logic serves the same purpose as discourse.rs's "AGREE valid only when citing new evidence (file:line)" rule — in the marketing domain, citing a passage/basis within the content instead of file:line could be considered as a substitute.

### Suggested next steps

- No precedent within the scope surveyed codifies the discourse rules (forced CHALLENGE + anonymization + mandatory new evidence) in marketing or adjacent domains → if marketing-loop implements this 3-stage structure, it may be the first such case in the market — but this is a fact confirmed within the surveyed scope, and is separate from the possibility that such precedent exists in unsurveyed territory.
- Review the legal MAD (arXiv:2606.30906) 3-Ply algorithm (with explicit model-call counts) as the primary reference template for discourse round design.
- Reference HAJailBench's quantitative termination conditions (similarity threshold, risk-band convergence, 3-round ablation) when designing discourse early-termination rules.
- CourtGuard, Debating Truth, FC-MAD/Tool-MAD, and AgentsBench remain unconfirmed in detail, so while lower priority, they are left as candidates for re-investigation if the original text/repo becomes accessible again.
- No case of a discourse structure in the advertising-review-board domain was found in this survey (unconfirmed) — whether a separate source investigation is warranted needs further judgment.
