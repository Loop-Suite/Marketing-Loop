# marketing-loop Design Spec

marketing-loop is a design that ports Code-Review-Loop's (a Rust-based persona-review CLI) 12-stage pipeline and persona discourse structure directly onto the marketing content review domain, with only the minimal changes unavoidable given domain differences.

---

## 0. Code-Review-Loop's 12-stage pipeline vs. the marketing-loop mapping

| Stage | Module | codereview original | marketing-loop substitution |
|---|---|---|---|
| Input normalization / convention injection | input.rs | diff + coding convention | Original content (`--content-type`) + brand voice/style guide injection. Normalizes content into block_id units (headline/subhead/body_N/cta/subject_line/meta_description/alt_text) — the counterpart to file:line, required for discourse evidence citation |
| Lens selection (3-5) | lens.rs::select_lenses | based on diff characteristics | based on `--content-type` |
| Deterministic vs. semantic split | report.rs::deterministic_table | — | same structure kept |
| Policy checks / binary verdicts | policy.rs | coding policy | binary gates such as ad disclosure, opt-out link, banned words |
| Per-lens independent review | lens.rs::review_lens | — | independent review per persona, same structure |
| Discourse debate | discourse.rs | AGREE/CHALLENGE/CONNECT/SURFACE | most rules ported as-is, some CHALLENGE conditions added |
| Requirement verification | requirements.rs | PR requirements | verifies whether the campaign brief / creative brief is met |
| Quantitative summarization | quantify.rs | P0=25/P1=12/P2=5/P3=1 | weight numbers kept identical, only severity definitions redefined for the domain |
| Prior-run fix check (--prior) | fixcheck.rs + state.rs | FIXED/STILL_OPEN/UNKNOWN | fixcheck.rs's verdict logic (FIXED/STILL_OPEN/UNKNOWN) is kept as-is. However, the state.json storage schema itself is substantially expanded from the original (State={round, findings, resolved}, 3 fields) — not the same structure as the original, see §6 |
| Human-voice rewrite | humanvoice.rs | — | same structure kept |
| Final report assembly | report.rs | — | same structure kept |

---

## 1. Marketing-domain personas (7)

| Lens | Persona (real) | Basis | Persona tone | Tier |
|---|---|---|---|---|
| brand_voice | David Ogilvy | Founder of Ogilvy & Mather, "The consumer isn't a moron, she is your wife" | Assertive, aphoristic tone; calls out exaggerated/gimmicky copy on sight; obsessed with tone consistency | 1 |
| conversion_cro | Robert Cialdini | *Influence*, the 6 principles of persuasion (reciprocity/scarcity/social proof/authority/consistency/liking) | Cites psychological grounding; structurally points out where persuasion principles are missing | 1 |
| seo | Rand Fishkin | Co-founder of Moz | Practical, checklist-style tone centered on search intent and keywords | 1 |
| audience_fit | Seth Godin | *Permission Marketing*, *Purple Cow*, *This Is Marketing* | Short and provocative; repeatedly asks "Is this remarkable? Did the audience give permission?" | 1 |
| claims_compliance | Claude C. Hopkins | *Scientific Advertising* (1923), originator of the provable reason-why advertising principle | Positivist; repeatedly asks "Is it provable? Is there a number?" | 1 |
| clarity_style | Ann Handley | MarketingProfs CCO, *Everybody Writes* | Editorial tone; flags clutter and passive voice sentence by sentence | 2 |
| cross_channel_consistency | Byron Sharp | Ehrenberg-Bass Institute, *How Brands Grow*, the distinctive brand assets theory | Dry, academic tone; points out drift in cross-channel asset consistency | 2 |

> **Assumption:** claims_compliance is not an actual legal expert but an approximate mapping of the "provable claims" philosophy onto a legal/compliance lens (Hopkins was a copywriter, not a lawyer). Actual regulatory-violation verdicts are handled not by the persona but by policy.rs's deterministic gate; the persona provides only qualitative risk commentary.
>
> **Assumption:** the original spec.rs's `tier` is of type `String`, and its source comment explicitly states "generalist | specialist | famous_engineer | custom. Display only, not involved in logic" — nowhere in lens.rs's lens-selection logic is `tier` referenced. In the original, forcing a lens to be included is handled not by `tier` but by a separate `always: bool` field, and in the actual default.toml only 2 of the 12 lenses (functionality/good_things) have `always = true`. Therefore, the integer/selection-priority meaning used here — tier=1 (5 required lenses, high priority) / tier=2 (auxiliary lenses, selected per content-type) — is a design extension newly given to marketing-loop that did not exist in the original, not a direct port of the original tier's meaning. To strictly follow the original structure, `tier` should remain a display-only string, and whether a lens is required should be managed via a separate `always: bool` field.

### Lens selection by content-type (3-5 of the 7-lens pool)

| --content-type | Selected lenses |
|---|---|
| ad_copy | brand_voice, conversion_cro, claims_compliance, audience_fit |
| landing_page | seo, conversion_cro, brand_voice, clarity_style, claims_compliance |
| email | conversion_cro, brand_voice, claims_compliance, audience_fit |
| social_post | brand_voice, audience_fit, cross_channel_consistency, claims_compliance |
| blog_post | seo, clarity_style, brand_voice, claims_compliance, audience_fit |

---

## 2. spec.toml example

```toml
[[persona]]
persona_name = "David Ogilvy"
persona_voice = "Assertive, aphoristic tone. Immediately flags exaggerated/gimmicky copy under the 'the consumer isn't a moron' principle. Obsessed with brand tone consistency."
lens = "brand_voice"
tier = 1

[[persona]]
persona_name = "Robert Cialdini"
persona_voice = "Cites psychological grounding. Structurally pinpoints where persuasion principles (reciprocity/scarcity/social proof/authority/consistency/liking) are missing."
lens = "conversion_cro"
tier = 1

[[persona]]
persona_name = "Rand Fishkin"
persona_voice = "Data and search-intent focused. Flags keyword/meta/structure issues in the tone of a practical SEO checklist."
lens = "seo"
tier = 1

[[persona]]
persona_name = "Seth Godin"
persona_voice = "Short, provocative sentences. Repeatedly asks 'Is this remarkable, is this a message this audience gave permission for?'"
lens = "audience_fit"
tier = 1

[[persona]]
persona_name = "Claude C. Hopkins"
persona_voice = "Positivist. Repeatedly asks 'Is it provable, is there a number?' Unforgiving toward exaggerated or unsupported claims."
lens = "claims_compliance"
tier = 1

[[persona]]
persona_name = "Ann Handley"
persona_voice = "Editorial tone. Flags clutter, passive voice, and jargon sentence by sentence, emphasizing the 'write like you're talking to one person' principle."
lens = "clarity_style"
tier = 2

[[persona]]
persona_name = "Byron Sharp"
persona_voice = "Dry, academic tone. Points to data showing drift in distinctive brand assets (logo/color/tagline) across channels."
lens = "cross_channel_consistency"
tier = 2
```

> The `tier=1/2` values above carry a different meaning from the original spec.rs's `tier: String` (display-only, not involved in selection logic) — this is a field newly defined in marketing-loop. See the assumption item in §1. To strictly follow the original schema, `tier` should remain a string label, and whether a lens is required should be managed as a separate `always: bool` field.

---

## 3. deterministic_checks list

| check_id | Description | Applicable content-type | Local tool/implementation |
|---|---|---|---|
| banned_words | Scans for banned words, competitor names, and exaggerated superlatives | all | Custom implementation (wordlist + regex) |
| legal_claim_scan | Detects absolute/medical/financial efficacy claims | all | Custom implementation |
| required_disclaimer_check | Presence of ad/sponsorship disclosure, opt-out link | email, social_post | Custom implementation |
| channel_length_limit | Per-channel character-count limit | ad_copy, social_post, landing_page | Custom implementation (counter) |
| readability_score | Flesch-Kincaid / Flesch Reading Ease | blog_post, landing_page, email | textstat (Python) / readability-cli (npm) — existing tool |
| spelling_grammar_check | Spelling/grammar errors | all | LanguageTool (open source, self-host) — existing tool |
| brand_keyword_match | Presence of required brand/product-name keywords | all | Custom implementation |
| trademark_symbol_check | Missing ®/™ marks | all | Custom implementation |
| link_url_validity | Checks in-body URL response codes and UTM parameters | email, landing_page, social_post | linkinator (npm) — existing tool |
| pii_scan | Scans for accidental inclusion of personal information (secrets-scan counterpart) | all | Custom implementation (regex, reusing gitleaks patterns) |
| duplicate_content_check | Duplication against previously published content (SEO cannibalization) | blog_post, landing_page | Custom implementation (simhash) |
| accessibility_contrast | WCAG color contrast (CTA buttons, etc.) | landing_page | pa11y / axe-core — existing tool |

**Structural difference in the semgrep counterpart:** in the original, a single `semgrep --config=auto` automatically covers SAST/secrets. There is no such single all-purpose scanner in the marketing domain — LanguageTool covers only spelling_grammar_check on its own, and everything else needs to be individually combined. It must be explicitly recognized as a difference that no auto-fill default (semgrep's counterpart) exists.

---

## 4. Porting judgment for discourse.rs

| Original rule | Validity in the marketing domain | Judgment |
|---|---|---|
| Strips reviewer identity, leaving only id/file:line/claim/evidence | Valid | Ported as-is. file:line → block_id:offset (e.g. cta_1:0, headline:0) |
| AGREE is valid only when citing new evidence not already in the existing finding | Valid | Ported as-is. AGREE holds only when "the same issue is reconfirmed in a different block" |
| At least 1 CHALLENGE forced per round; if not met, 1 automatic retry | Conditionally valid — needs modification | The anti-groupthink purpose matters even more in a domain with heavier subjective judgment, so the rule is kept. However, the CHALLENGE-validity criteria need an added distinction between "evidence-based rebuttal vs. pure difference in taste." E.g., a taste-based objection like "it'd read better if the tone were a bit more casual" should not count as a valid CHALLENGE — only rebuttals grounded in policy/data/brand guide should count. Without this distinction, tone disputes would force a retry every round, adding noise without value. |
| CONNECT (relates to a finding in another lens) | Valid | Ported as-is. E.g., linking an SEO-lens keyword-shortage finding to a Clarity-lens readability finding |
| SURFACE (raises a new issue) | Valid | Ported as-is |

> **Assumption:** adding the CHALLENGE condition is a design judgment (concern over subjective tone-dispute noise given the domain's character) with no basis in the original README — it is not an unverified extension, but a minimal correction scoped to the defect that would otherwise arise from a direct port.

---

## 5. CLI subcommand mapping

| Subcommand | Code-review original | marketing-loop counterpart | 1:1? |
|---|---|---|---|
| review | diff/spec/requirements/conventions/deterministic-results → report.md+state.json | original content/spec/campaign brief (requirements counterpart)/brand style guide (conventions counterpart)/deterministic-results → report.md+state.json | 1:1, only input names substituted |
| describe | PR summary: title/summary/walkthrough/labels/can_be_split/TODO scan | content summary: title/core message/target audience & tone/variant guidance/labels (channel, campaign stage)/can_be_split (whether it can be split into smaller units)/TODO scan ([TBD], lorem ipsum, placeholder text) | 1:1 |
| improve | before/after patch suggestions | before/after copy rewrite suggestions | 1:1, only "patch" → "copy alternative" substituted |
| ask | free-form Q&A, accumulated into ask.md | free-form Q&A (e.g. "will this copy resonate with Gen Z?"), accumulated into ask.md | 1:1, unchanged |

All 4 subcommands can be carried over with only the input domain substituted, no structural change. No new subcommand is added.

---

## 6. Output schema (report.md / state.json)

### report.md fields

- **verdict** (PASS/REVISE — assumption: the exact formula for deriving the original verdict is not in the README, inferred as a policy-fail-override approach; not confirmed)
- **policy checks** (list of binary pass/fail)
- **findings** (persona/severity/block location/claim/evidence)
- **good things**
- **deterministic checks** (status/evidence per check_id)
- **discourse audit** (per-round AGREE/CHALLENGE/CONNECT/SURFACE log)
- **requirements verification** (counterpart to the original report.rs's `## Requirements Verification` section — whether the campaign/creative brief is met)
- **human-voice review** (counterpart to the original report.rs's `## Human-voice Review` section — attached only when `--human-voice` is specified)
- **comparison to prior round** (counterpart to the original report.rs's `## Comparison to Prior Round` section — only when `--prior` is specified, a list of FIXED/STILL_OPEN/UNKNOWN)

### state.json schema

> The schema below does not reuse the original state.rs's 3-field `State { round, findings, resolved }` structure as-is; it is intentionally expanded with fields needed to reconstruct the report (run_id/content_type/timestamp/verdict/score/policy_checks/lens_selected/discourse_log/good_things/prior_ref, etc.). It is explicitly noted that this is not "the same structure" as the original, but a newly designed schema that references the original's minimal-snapshot concept. The FIXED/STILL_OPEN/UNKNOWN values in `findings[].status` are, in the original, a transient fixcheck.rs re-verdict result that is not persisted to state.json permanently, and is a separate concept from the discourse Resolution's own values (CONFIRMED/REJECTED/MERGED/UNCERTAIN).

```json
{
  "run_id": "string",
  "content_type": "ad_copy|landing_page|email|social_post|blog_post",
  "timestamp": "ISO8601",
  "verdict": "PASS|REVISE",
  "score": 0,
  "policy_checks": [{"check_id": "string", "status": "PASS|FAIL", "evidence": "string"}],
  "deterministic_checks": {"check_id": {"status": "PASS|FAIL|WARN", "evidence": "string"}},
  "lens_selected": ["brand_voice", "conversion_cro"],
  "findings": [{"id": "string", "lens": "string", "persona": "string", "severity": "P0|P1|P2|P3", "block_ref": "block_id:offset", "claim": "string", "evidence": "string", "status": "FIXED|STILL_OPEN|UNKNOWN"}],
  "discourse_log": [{"round": 0, "tag": "AGREE|CHALLENGE|CONNECT|SURFACE", "persona": "string", "target_finding_id": "string", "evidence": "string"}],
  "good_things": ["string"],
  "prior_ref": "path|null"
}
```

### Severity weights

The quantify.rs hardcoded values P0=25/P1=12/P2=5/P3=1 are kept exactly as-is (no reinterpretation). Only the domain-specific severity definitions are reinterpreted:

| Severity | Marketing-domain definition |
|---|---|
| P0 | Legal/compliance violation, brand-safety risk — cannot ship |
| P1 | Serious defect that undermines conversion/trust, potentially misleading claim |
| P2 | Tone/consistency/readability issue |
| P3 | Minor style preference |

> **Assumption:** these severity definitions are a design judgment; the exact definition of P0-P3 in the original code domain is not in the README and cannot be confirmed — only the numeric weights are kept identical to the original.
