# Empirical review findings — codereview pipeline validation

This records what actually happened when the `codereview` binary in this repo (a port of
[Code-Review-Loop](https://github.com/Loop-Suite/Code-Review-Loop)'s persona+discourse review
architecture to a marketing-content review domain) was reviewed and then actually run. Two passes
were pure static code review — no LLM calls, no cost. A third pass ran the real CLI
(`codereview review`, `claude -p --model haiku`, real API cost) against a synthetic ad-copy fixture
to check whether the fixes held up outside of reading the code, including chaining rounds together
with `--prior`.

Every number below is either read directly off the CLI's own console cost line or computed from a
saved `report.md`, with labeled exceptions in the cost table. Nothing here is estimated from
memory.

A later **production-hardening round** added a fourth, adversarial static pass (#17–#19); this
repo's first unit test suite (73 tests, #23); a tagged `v0.1.0` release (`CHANGELOG.md`, #24); and
a second real-execution pass (#25, #27) that specifically re-validated the fourth pass's fixes
against actual model output, not just a unit test. See "Round 4" onward below.

## TL;DR

| Pass | Method | Issues found | Cost |
|---|---|---|---|
| Round 1 — initial static review | Code reading only | 4 (#2, #3, #4, #5) | $0 |
| Round 2 — deeper static review | Code reading only, closer second pass | 3 (#6, #7, #8) | $0 |
| Round 3 — real CLI execution review | 4 successful `codereview review` runs + 3 crash-and-retry runs while triaging | 2 (#9, #10) | ~$1.0–$1.2 (uncertain — see Cost) |
| Round-chaining validation | Same 4 real runs as Round 3 | 0 new issues — confirms #2/#4/#6/#8 hold under real chained `--prior` use | (included above) |
| **Subtotal — initial validation pass** | | **9 issues opened, all closed** | **~$1.0–$1.2** |
| Round 4 — adversarial re-audit | Code reading only, third static pass, deliberately re-opening prior rounds' open questions | 3 (#17, #18, #19) | $0 |
| Unit test suite | No new execution — 73 unit tests added, all prior numbered fixes (#2, #3, #5, #6, #9, #10, #17, #18, #19) covered as regressions | 0 new issues | $0 |
| Round 5 — real execution validation, take 2 | 2 chained `codereview review` runs (`--backend claude-cli --model haiku`) + 1 crash-and-fix cycle | 2 (#25, #27) | $0.7754 |
| **Grand total — all rounds** | | **14 issues opened, all closed** | **~$1.78–$1.98** |

**What this bought:**

- **Two free static passes caught 7 of the 9 bugs before a single dollar was spent**: #2
  (`state::load` crashes with "Is a directory" when `--prior` points at a directory), #3
  (`fixcheck::run` never receives the revised content, so FIXED/STILL_OPEN verdicts are guessed
  blind), #4 (console prints the pre-merge verdict/score while `report.md` prints the post-merge
  one — same run, two different verdicts), #5 (a character-count policy limit compared against a
  word count), #6 (finding-id collision, see below), #7 (`requirements::verify` runs before the
  `--prior` merge), #8 (MERGED findings silently dropped from `report.md`).
- **The worst bug needed a second, harder look to surface.** #6 wasn't caught in round 1: a
  round's own local finding id could collide with a re-confirmed `--prior` finding's id, letting a
  `REJECTED` finding get silently resurrected as `CONFIRMED` and double-counted into the score.
  Reproduced concretely: a run that should have scored 95 scored 90 instead.
- **Running the real binary against real content — not just reading the diff — found two bugs
  static review missed.** #9: an LLM response containing an explicit JSON `null` (not just a
  missing field, which `#[serde(default)]` already covered) crashed the entire process and
  discarded every finding already computed in that run, in 6 files sharing the same
  deserialization pattern (`discourse.rs`, `lens.rs`, `describe.rs`, `requirements.rs`,
  `fixcheck.rs`, `improve.rs`) — fixed once with a shared `null_as_default` deserializer instead
  of 6 separate patches.
- **#10 is the one worth reading twice: the #8 fix had never actually worked.** #8 made MERGED
  findings render in `report.md` — but `CONNECT` moves were supposed to merge findings in the
  first place, and they never had, in any run, because the model returns multiple target finding
  ids joined by commas (e.g. `"claims_compliance-3,copy_craft-1"`) into `target_finding_id`, a
  field typed as a single `String`. The match always failed silently, so nothing was ever merged.
  The section existed and rendered correctly from the moment #8 landed — it just never had
  anything in it until #10 fixed the actual merge.
- **Round-chaining was validated on real executions, not just unit tests**: `--prior` given a
  directory path succeeded twice for real; a re-confirmed prior finding coexisted with the new
  round's own same-named finding without collision (via a `prior-` id prefix); the Merged Findings
  section rendered non-empty content in every one of the 4 real runs; console verdict/score
  matched `report.md`'s verdict/score exactly in every run where both were captured.
- **One thing was found and deliberately left alone**: `MERGED`-status findings are invisible to
  `quantify::score`/`verdict`, which only aggregates `CONFIRMED` findings — observed directly on
  real runs, not hypothesized. See "Known limitation" below; not filed as an issue because it's
  genuinely unclear whether that's a bug or the intended design.

## Round 1 — initial static review (#2–#5)

Read-only review of the ported pipeline before any execution. All four fixed together in commit
`1c70581` (`src/state.rs`, `src/fixcheck.rs`, `src/main.rs`, `src/policy.rs`).

- **[#2](https://github.com/Loop-Suite/Marketing-Loop/issues/2) — `state::load` crashes with "Is a
  directory" when `--prior` is given a directory path.** The loader assumed `--prior` always names
  a file; passing the run directory it's documented to accept (i.e. the natural output of a
  previous run) crashed instead of resolving to `report.md`/`state.json` inside it.
- **[#3](https://github.com/Loop-Suite/Marketing-Loop/issues/3) — `fixcheck::run` never receives
  the revised content, so FIXED/STILL_OPEN verdicts are guessed blind.** The revised
  text/diff was never actually passed to the fix-check LLM call — its FIXED/STILL_OPEN
  determination had nothing but the prior finding to go on.
- **[#4](https://github.com/Loop-Suite/Marketing-Loop/issues/4) — console output reports the stale
  pre-merge verdict/score while `report.md` reports the post-merge one — same run, two different
  verdicts.** `main.rs` printed the verdict computed before the `--prior` merge; `report.md` was
  generated after it. Same invocation, two disagreeing answers depending on where you looked.
- **[#5](https://github.com/Loop-Suite/Marketing-Loop/issues/5) —
  `content_length_check` compares `word_count` against a documented character-count limit.** The
  policy check counted words but compared the result against a limit documented and configured as
  a character count — wrong unit, not just a wrong number.

## Round 2 — deeper static review (#6–#8)

A second, closer pass over the same code — specifically how a round's own findings interact with
re-confirmed `--prior` findings — turned up three more bugs the first pass missed.

- **[#6](https://github.com/Loop-Suite/Marketing-Loop/issues/6) — finding-id collision between a
  round's own findings and re-confirmed `--prior` findings resurrects REJECTED findings as
  CONFIRMED and double-counts score.** The most severe finding of the whole review. If a round's
  own local finding id happened to match a re-confirmed prior finding's id, merging the two id
  spaces let a `REJECTED` finding overwrite/be overwritten by a `CONFIRMED` one under the same
  key, silently promoting it and double-counting its score deduction. Reproduced concretely: a
  run that should have scored 95 scored 90 instead. Fixed in `fd5408e` (`src/main.rs`) by giving
  prior findings a `prior-` id prefix so the two spaces can never collide — this is the same
  prefix visible in the real round-3 output below.
- **[#7](https://github.com/Loop-Suite/Marketing-Loop/issues/7) — `requirements::verify` runs
  before the `--prior` merge, so re-confirmed findings are invisible to the
  requirements-verification LLM call.** This re-opens what #4 left incomplete: #4 fixed the
  *verdict/score* to reflect the post-merge state, but the requirements-verification call itself
  still ran on the pre-merge finding set, so it was evaluating requirements against a finding list
  that hadn't been merged yet. Fixed in `30538d3` (`src/main.rs`) by moving the call after the
  merge.
- **[#8](https://github.com/Loop-Suite/Marketing-Loop/issues/8) — MERGED findings (from a
  discourse CONNECT) are completely dropped from `report.md`.** A finding that discourse decided
  to `CONNECT` (merge) into another simply disappeared from the report — no record it existed, no
  record of what it was merged into. Fixed in `0825362` (`src/report.rs`) by adding a "Merged
  Findings" section. (See #10 below: this fix rendered correctly from day one, but had nothing to
  render until a second, unrelated bug was also fixed.)

## Round 3 — real CLI execution review (#9–#10)

Static review can only catch what's visible in the code. These two were found by actually running
`codereview review` with `--backend claude --model haiku` against real content and reading what
came back — including from runs that crashed outright.

- **[#9](https://github.com/Loop-Suite/Marketing-Loop/issues/9) — LLM-returned explicit JSON
  `null` crashes the whole run, discarding all completed findings.** `#[serde(default)]` only
  covers a *missing* field; when the model returns the field explicitly as JSON `null` (which it
  does, in practice), deserialization still fails, and the whole process aborts — losing every
  finding already computed earlier in that same run, not just the malformed response. Confirmed
  as a real, non-hypothetical failure mode by actual crash logs during triage (one such log
  captured a `missing field 'target_finding_id'` schema mismatch on a discourse `SURFACE` move,
  which legitimately has no target — the same family of "the model's JSON doesn't match the
  struct's strictness" failure). The same pattern existed independently in 6 files:
  `discourse.rs`, `lens.rs`, `describe.rs`, `requirements.rs`, `fixcheck.rs`, `improve.rs`. Fixed
  once in `a3a68a5` with a shared `null_as_default` deserializer in `src/llm.rs`, applied at each
  call site instead of patched 6 separate times.
- **[#10](https://github.com/Loop-Suite/Marketing-Loop/issues/10) — CONNECT moves never actually
  merge findings — `target_finding_id` can't hold multiple ids.** Discovered while re-verifying
  #8 against real output: the Merged Findings section #8 added was rendering, but consistently
  empty or missing entries a `CONNECT` move should have produced. Root cause: the model reliably
  returns multiple target ids as one comma-joined string (real example from this repo's own
  output: `"claims_compliance-3,copy_craft-1"`), but `target_finding_id` is typed as a single
  `String`, so the id lookup never matched and the merge silently no-opped. **Practical
  consequence: #8's fix had never once actually fired on real output before this was found** —
  the rendering path was correct and untested-but-working; the data path feeding it had been
  broken the entire time. Fixed in `4db0647` (`src/discourse.rs`) by splitting on commas.

## Round-chaining validation (real executions)

The 4 successful CLI runs below happened after every fix above (`#2`–`#10`) had already landed —
their purpose was to confirm the fixes hold together under a real, chained `--prior` workflow, not
just individually. All four reviewed the same synthetic ad-copy fixture (`content_type: ad_copy`,
a fitness-app ad with a headline/body/CTA), spec `specs/default.toml`.

| Run | Round | `--prior` | Console verdict/score | Console cost | `report.md` verdict/score |
|---|---|---|---|---|---|
| round1 | 0 | — | APPROVE 100/100 (3 calls) | $0.1481 | APPROVE 100/100 — matches |
| round2 | 1 | round1's directory | APPROVE 100/100 (3 calls) | $0.1511 | APPROVE 100/100 — matches |
| round1_synth | 0 | — (synthetic prior fixture, built to carry a `CONFIRMED` finding into round3) | not captured to a saved log | not captured to a saved log | APPROVE 100/100 |
| round3 | 1 | round1_synth's directory | REQUEST_CHANGES 75/100 (4 calls) | $0.1941 | REQUEST_CHANGES 75/100 — matches |

What each row actually demonstrates, checked against the saved `report.md`/console output rather
than assumed:

- **`--prior` on a directory path (#2) worked twice for real**: round2 was pointed at round1's run
  directory, round3 at round1_synth's — both resolved and ran, not just the file-path case.
- **Finding-id collision avoidance (#6) is visible in round3's own output.** Its Findings table
  carries `prior-copy_craft-1` (P0, re-confirmed from round1_synth, `STILL_OPEN`) as a distinct
  row, while the same round's *own* newly-detected findings — `claims_compliance-1..4` and
  `copy_craft-1..4` — appear separately in that round's Merged Findings section. The prior id and
  the round-local id both named `copy_craft-1` coexist without collision, which is exactly the
  scenario #6 broke.
- **The Merged Findings section (#8) rendered non-empty content in every run**: 6 entries in
  round1, 4 in round2, 8 in round3 — not just an empty header, real cross-lens-merged findings
  with their own evidence text.
- **Console verdict/score matched `report.md`'s verdict/score exactly** in round1, round2, and
  round3 — the three runs where both were captured to a file. round1_synth's report is internally
  consistent (APPROVE 100/100) but its console output wasn't saved to a separate log, so that one
  run's match isn't independently file-verifiable here, only observed live during the session.

## Known limitation — MERGED findings were invisible to scoring (resolved in #17)

**Resolved.** This was originally recorded below as a deliberate non-fix — "genuinely unclear
whether this is a bug or the intended design," not filed as an issue. The production-hardening
round's adversarial re-audit (see "Round 4" below) revisited it and decided: blocking-tier
findings (`tier = "blocking"` in the spec, e.g. `claims_compliance`) now count toward
score/verdict even when discourse `CONNECT`s them into another finding; non-blocking-tier
findings keep the original consolidation-only behavior. Fixed in #17/#20, confirmed against real
model output (not just a unit test) in "Round 5" below. Original record of the gap, kept verbatim
below for the trade-off reasoning it documents:

`quantify::score`/`verdict` only aggregate findings with status `CONFIRMED`. A finding with status
`MERGED` (the outcome of a discourse `CONNECT`, see #8/#10 above) contributes nothing to the score
and nothing to the verdict — it's fully visible in the report (that's what #8 fixed) but
functionally invisible to the numbers that decide APPROVE vs. REQUEST_CHANGES.

This is not hypothetical — it happened on the very first real run in this review, unprompted:
round1's report shows `claims_compliance` and `copy_craft` both independently flagging real,
substantive problems in the fixture copy — `"guaranteed to transform your body in 2 weeks"`,
`"#1 fitness app in the world"`, `"never miss a workout anymore"` — exactly the class of
unsubstantiated efficacy/superlative/guarantee claims `claims_compliance` exists to catch (per
`specs/default.toml`, `claims_compliance` is `tier = "blocking"` and `always = true`). Discourse
`CONNECT`ed all of them into the Merged Findings section as cross-lens-confirmed. None reached
`CONFIRMED`. The result: `"No deductions (no CONFIRMED findings)"`, **score 100/100, verdict
APPROVE** — on copy making three separate false/unsubstantiated guarantee claims.

Why this wasn't just fixed alongside #6/#8: it's genuinely ambiguous whether this is a bug or
intended behavior. Two readings both seem defensible:

- **Bug reading**: a `CONNECT` is a *consolidation* of duplicate findings, not a *dismissal* — the
  underlying defect is still real and still unaddressed, so at least one merged finding's severity
  should still count toward the score.
- **By-design reading**: `CONNECT` may be intended to mean "this is redundant with a finding
  that's being scored elsewhere," and the actual gap is that a `claims_compliance` (blocking-tier)
  finding should never have been eligible for `CONNECT` in the first place, rather than that
  `MERGED` findings should count toward scoring generally.

Both point at real, different fixes, and picking wrong risks either double-counting merged
duplicates or breaking the discourse consolidation mechanism that #10 just made actually work.
Recorded here as a real, measured gap rather than guessed at or silently patched.

## Round 4 — adversarial re-audit (#17–#19)

A third static-review pass, adversarial in intent: re-reading the pipeline specifically looking
for what earlier passes had rationalized away, left as an open question, or not looked at from a
security angle. Three issues found, each fixed in its own PR: #20 (`e444169`), #21 (`5f246dd`),
#22 (`38d9e4e`).

- **[#17](https://github.com/Loop-Suite/Marketing-Loop/issues/17) — the "Known limitation" above
  was actually decided, not just recorded.** Round 3 found that a `tier = "blocking"` finding
  (e.g. `claims_compliance`) merged via discourse `CONNECT` was invisible to
  `quantify::score`/`verdict`, letting a real, unaddressed compliance violation resolve to
  `APPROVE`. Left open at the time because two readings were both defensible (see "Known
  limitation" above). This pass decided it: `quantify::counts_toward_score` now also counts
  `MERGED` findings whose originating lens is blocking-tier; non-blocking-tier `MERGED` findings
  keep the original consolidation-only behavior. **Trade-off made explicit, not hidden**: if
  multiple blocking-tier findings `CONNECT` into each other, each one's severity is still
  deducted independently — no de-duplication of the penalty across a merge chain. Chosen
  deliberately: false leniency (a real compliance violation silently scoring 100/100 because it
  happened to get merged) is judged worse than a possibly-inflated deduction. Fixed in
  `e444169`/#20.
- **[#18](https://github.com/Loop-Suite/Marketing-Loop/issues/18) — the same crash class as #9,
  on fields #9 never touched.** #9 fixed "explicit JSON `null` crashes the whole run and discards
  every finding already computed," but only for fields that already had a bare
  `#[serde(default)]`. Fields with no `#[serde(default)]` at all — still `required`, and still
  exposed to the identical crash — were outside #9's fix scope: `Move.kind`, a raw finding's
  severity/label/claim/evidence/block_ref, `describe.rs`'s
  title/summary/walkthrough/can_be_split, `improve.rs`'s `Suggestion` (6 fields), `fixcheck.rs`'s
  `FixResult` (finding_id/status/evidence), and `requirements.rs`'s `RequirementCheck`
  (requirement/status/evidence) — 6 structs across 6 files. Fixed once in `5f246dd`/#21 by
  applying the same `null_as_default` deserializer #9 established, instead of 6 separate patches.
- **[#19](https://github.com/Loop-Suite/Marketing-Loop/issues/19) — the marketing copy under
  review was spliced into every LLM prompt with no framing distinguishing it from instructions.**
  The content being reviewed — authored by the party with the strongest incentive to get an
  `APPROVE` — was embedded verbatim in every prompt, with no delimiter or note marking it as data
  rather than instructions: a real prompt-injection surface, not a hypothetical one, given this
  tool exists specifically to catch problems in that same copy. Fixed in `38d9e4e`/#22 by adding
  an explicit note ahead of the content block(s) at both call sites (`shared_context` and
  fixcheck's hand-built context): this is untrusted material submitted for review, and any
  embedded instruction-like text inside it is itself grounds for a finding, never something to
  obey. Defense-in-depth — explicitly not a complete prompt-injection defense.

## Unit test suite (#23)

This repo had zero tests before this round. #23 (`589ba02`) added 71 unit tests in one PR
covering `state.rs`, `quantify.rs`, `checks.rs`/`policy.rs`, and `discourse.rs`'s core logic, plus
explicit-JSON-null deserialization tolerance on every LLM-facing struct. Two more tests were added
incidentally by the fixes below (#26, #28), bringing the current total to **73**.

Every numbered issue from both this round and the original validation pass that's expressible
without mocking the `Llm` subprocess boundary is now covered as a regression test: #2, #3, #5,
#6, #9, #10, #17, #18, #19. To make this possible without changing behavior, two pure helpers were
extracted — `fixcheck::build_ctx` (fixcheck's prompt-context construction) and
`main::prior_finding_id` (the `--prior` id-namespacing logic behind #6's fix) — no behavior
change, existing logic made directly testable.

#4 and #7 are explicitly out of scope for direct testing: both are `main.rs` call-ordering bugs
(console vs. `report.md` computed from different snapshots; `requirements::verify` running before
the `--prior` merge) that would need mocking the `Llm` subprocess/network boundary to exercise
directly. Covered indirectly instead: `quantify.rs`'s `score_before_and_after_prior_merge_can_differ`
test asserts the underlying invariant #4's fix depends on.

## Versioning — CHANGELOG.md and v0.1.0 (#24)

#24 (`67d90fd`) added `CHANGELOG.md` (Keep a Changelog format), covering every fix from #2 through
#19, and the repository was tagged
[`v0.1.0`](https://github.com/Loop-Suite/Marketing-Loop/releases/tag/v0.1.0).

**The tag is stale relative to `main` as of this writing.** `git log v0.1.0..main` shows two
commits not in the tagged release:

```
34ed6f3 Use counts_toward_score for --prior fix-check carry-over (#28)
75981fe Pass content_type into lens selection (#26)
```

`75981fe` (#26, fixes #25) and `34ed6f3` (#28, fixes #27) both landed *after* the tag was cut, as
part of the execution-validation pass below. **`v0.1.0` does not include the lens-selection
content-type fix (#25) or the `--prior` cross-round tracking fix (#27)** — both exist only on
`main`. Anyone deploying from the `v0.1.0` tag rather than `main` HEAD is running code with the
#25 crash and the #27 tracking gap still present.

## Round 5 — real execution validation, take 2 (#25, #27)

A second real-execution pass, chaining two rounds together with `--prior` against
`examples/sample_ad_copy.md` (`--backend claude-cli --model haiku`), run specifically to check
whether #17's fix (Round 4, above) holds up on real model output and not just in a unit test.

- **Round A crashed on the first attempt — before #17 could even be exercised.**
  `lens::select_lenses`'s own prompt asks the model to pick lenses fitting "the content type and
  its nature," but the function has no `content_type`/`Input` parameter at all — `main.rs` had
  the value available and never passed it through. `haiku` didn't paper over the gap the way
  whatever model ran the earlier rounds apparently had: it noticed the prompt referenced
  information it was never given and refused to guess, surfacing as a hard `Lens selection
  failed` error instead of a silently arbitrary lens selection. Filed as
  [#25](https://github.com/Loop-Suite/Marketing-Loop/issues/25), fixed in `75981fe`/#26 (passes
  `content_type` through, extracts `build_selection_task()` so the prompt construction is
  directly unit-tested), rebuilt, rerun.
- **Round A, rerun, succeeded and validated #17 on real output.** Score 76/100 (verdict COMMENT).
  Two `claims_compliance` findings (`claims_compliance-1`, `claims_compliance-2`, both P1,
  flagging unsubstantiated efficacy claims in the fixture's headline/body) were discourse-
  `CONNECT`ed into each other in discourse round 2 — and, per #17's fix, both still appear in the
  main Findings table and both still deduct from the score (-12 each: 100 → 76), because
  `claims_compliance` is `tier = "blocking"`. In the same run, non-blocking-tier `CONNECT`s
  (`conversion_cro`/`copy_craft`/`brand_voice` findings merged over a placeholder-text defect and
  an unsupported-statistic defect) behaved exactly as before #17: visible only in the Merged
  Findings section, no score impact. Both halves of #17's trade-off confirmed on a real model
  output, not just a unit test.
- **Round B (`--prior` round A's directory, same unmodified content) also succeeded**: score
  63/100, verdict REQUEST_CHANGES, console and `report.md` verdict/score matched exactly.
- **Round B surfaced a new bug: #27.** Round A's two blocking-tier `claims_compliance` findings
  scored via `MERGED` status, not `CONFIRMED` — but `main.rs`'s `--prior` fix-check carry-over
  filter still checked for the literal string `"CONFIRMED"`, so those findings were invisible to
  `fixcheck::run` in round B: never checked for FIXED/STILL_OPEN, and no `## Vs. previous round`
  section rendered at all, despite round A having two real, score-affecting compliance findings
  that should have carried forward. Round B's own independent lens review happened to re-flag the
  same underlying defects on its own (hence it still landed on REQUEST_CHANGES) — but that's
  luck, not the tracking mechanism working; a revision that reworded around fresh detection while
  leaving the actual violation unaddressed would have lost continuity entirely. Filed as
  [#27](https://github.com/Loop-Suite/Marketing-Loop/issues/27), fixed in `34ed6f3`/#28 by
  swapping the literal status check for `quantify::counts_toward_score` (the same predicate #17
  added), covered by a regression test. **No further paid run was made to re-validate #27's
  fix** — the fix is unit-tested but not re-confirmed against another live chained run.

**Cost**: 2 rounds, 15 LLM calls total, **$0.7754** (round A $0.3907, round B $0.3847), both
`--backend claude-cli --model haiku`. Read directly off console output during the session; not
saved to a log file in this repo snapshot (same caveat as round1_synth's cost below).

## Cost

| Item | Calls | Cost |
|---|---|---|
| round1 | 3 | $0.1481 |
| round2 | 3 | $0.1511 |
| round1_synth | not logged | not logged |
| round3 | 4 | $0.1941 |
| **4 successful runs, total (direct console readout)** | | **$0.7205** |
| *(round1_synth's individual cost, by subtraction — not independently logged)* | | *($0.2272)* |
| 3 crash-and-retry runs while triaging #9/#10 | unknown | not printed (crashed before the cost line) |
| **Overall estimate, initial validation pass, all 7 attempts** | | **~$1.0–$1.2 (uncertain)** |
| round A (production-hardening, take 2) | part of 15 total | $0.3907 |
| round B (production-hardening, take 2) | part of 15 total | $0.3847 |
| **Production-hardening round 5, total (2 rounds)** | **15** | **$0.7754** |
| **Grand total, all execution-validation spend, both passes** | | **~$1.78–$1.98** |

All calls used `--model haiku` (initial pass: `--backend claude`; round 5: `--backend
claude-cli`). The $0.7205 total for the first pass's 4 successful runs is an exact sum of
console-reported costs; the $0.7754 total for round 5 is read
directly off console output during the session (not saved to a log file in this repo snapshot).
The 3 crash runs from the initial pass never reached the point where the CLI prints its cost
line, so their contribution to that subtotal is an estimate, not a measurement — stated as a
range rather than a false-precision single number.
